//! What the Jedi AI ([`crate::npc_jedi`] and its parts) reads of other clients and of the
//! rest of the game: another client as its AI sees it ([`JediClient`] — an NPC's own
//! record, a player's through [`NpcHost::jedi_player`]), and the NPC's saber and Force
//! switches it calls.

use crate::force_powers::{FP_ABSORB, FP_PROTECT, FP_RAGE};
use crate::npc_jedi_combat::ClientView;
use crate::npc_jedi_distance::JediFoe;
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// `HANDEXTEND_JEDITAUNT`; `ps.forceHandExtend`, `ps.saberHolstered`, `fd.forceGripCripple`.
const HANDEXTEND_JEDITAUNT: u8 = 16;
const PS_FORCE_HAND_EXTEND: usize = 80;
const PS_SABER_HOLSTERED: usize = 81;
const PS_GRIP_CRIPPLE: usize = 111;
/// `CHAN_WEAPON`.
const CHAN_WEAPON: u32 = 2;

/// What the evasions read of an enemy's client (`gclient_t` and `gentity_t` fields).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EnemyCombat {
    /// `r.currentOrigin`, `health`, `s.weapon`.
    pub origin: [f32; 3],
    pub health: i32,
    pub weapon: i32,
    /// `ps.viewangles`.
    pub view_angles: [f32; 3],
    /// `ps.weaponTime`, `ps.weaponstate`.
    pub weapon_time: i32,
    pub weapon_state: u8,
    /// `ps.fd.forcePowersActive`.
    pub force_powers_active: u32,
    /// `ps.saberLockTime`, `ps.saberInFlight`, `ps.saberEntityNum`, `ps.saberEntityState`.
    pub saber_lock_time: i32,
    pub saber_in_flight: bool,
    pub saber_entity_num: u16,
    pub saber_entity_state: i32,
    /// `painDebounceTime`.
    pub pain_debounce_time: i32,
    /// `renderInfo.muzzlePoint`, `renderInfo.muzzlePointOld`.
    pub muzzle_point: [f32; 3],
    pub muzzle_point_old: [f32; 3],
}

/// One blade of an enemy's saber (`client->saber[n].blade[m]`) as `Jedi_SaberBlock` reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EnemyBlade {
    /// `muzzlePoint`, `muzzleDir`, `muzzlePointOld`, `muzzleDirOld`, `length`.
    pub muzzle_point: [f32; 3],
    pub muzzle_dir: [f32; 3],
    pub muzzle_point_old: [f32; 3],
    pub muzzle_dir_old: [f32; 3],
    pub length: f32,
}

/// `s.pos.trDelta`, `s.weapon` (wire fields).
const ES_POS_DELTA: [usize; 3] = [6, 7, 10];
const ES_WEAPON: usize = 14;

/// Where an entity is and how it moves (`r.currentOrigin`, `s.pos.trDelta`, `s.weapon`):
/// a missile or a thrown saber coming at a Jedi ([`NpcHost::entity_motion`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EntityMotion {
    pub origin: [f32; 3],
    pub delta: [f32; 3],
    pub weapon: i32,
}

/// One blade of a client's saber as the AI reads it (`client->saber[n].blade[m]`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JediBlade {
    /// `muzzlePoint`, `muzzleDir`, `muzzlePointOld`, `muzzleDirOld`. (Its `length` is
    /// always 0: the multiplayer game never sets a blade's length but to zero it,
    /// `WP_SaberParseParms`, `bg_saberLoad.c:2201`.)
    pub point: [f32; 3],
    pub dir: [f32; 3],
    pub point_old: [f32; 3],
    pub dir_old: [f32; 3],
}

/// Another client (a player or an NPC) as the Jedi AI reads it: its entity's and its
/// client's fields.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JediClient {
    /// Its entity number.
    pub number: u16,
    /// `r.currentOrigin`, `r.mins`, `r.maxs`, `r.currentAngles`.
    pub origin: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub current_angles: [f32; 3],
    /// `health`, `enemy`, `painDebounceTime`, `attackDebounceTime`.
    pub health: i32,
    pub enemy: Option<u16>,
    pub pain_debounce_time: i32,
    pub attack_debounce_time: i32,
    /// `ps.viewangles`, `ps.velocity`, `ps.legsAnim`, `ps.groundEntityNum`.
    pub view_angles: [f32; 3],
    pub velocity: [f32; 3],
    pub legs_anim: u16,
    pub ground_entity: u16,
    /// `ps.weapon` (`s.weapon`), `ps.weaponTime`, `ps.weaponstate`.
    pub weapon: u8,
    pub weapon_time: i32,
    pub weapon_state: u8,
    /// `ps.saberMove`, `ps.saberLockTime`, `BG_SabersOff`, `ps.saberInFlight`,
    /// `ps.saberEntityNum`, `ps.saberEntityState`.
    pub saber_move: u32,
    pub saber_lock_time: i32,
    pub sabers_off: bool,
    pub saber_in_flight: bool,
    pub saber_entity_num: u16,
    pub saber_entity_state: i32,
    /// `ps.fd.forcePowersActive`, `ps.fd.forceGripBeingGripped`.
    pub force_powers_active: u32,
    pub grip_being_gripped: f32,
    /// `renderInfo.muzzlePoint`, `renderInfo.muzzlePointOld`.
    pub muzzle_point: [f32; 3],
    pub muzzle_point_old: [f32; 3],
    /// The first blade of each saber (the AI reads no other).
    pub blades: [JediBlade; 2],
}

impl JediClient {
    pub(crate) fn view(&self) -> ClientView {
        ClientView {
            number: self.number,
            origin: self.origin,
            mins: self.mins,
            maxs: self.maxs,
            current_angles: self.current_angles,
            velocity: self.velocity,
            legs_anim: self.legs_anim,
            ground_entity: self.ground_entity,
            saber_move: self.saber_move,
            saber_lock_time: self.saber_lock_time,
            weapon: self.weapon,
            sabers_off: self.sabers_off,
            force_powers_active: self.force_powers_active,
            attack_debounce_time: self.attack_debounce_time,
            enemy: self.enemy,
            health: self.health,
        }
    }

    fn foe(&self) -> JediFoe {
        JediFoe {
            client: true,
            origin: self.origin,
            weapon: i32::from(self.weapon),
            ground: self.ground_entity,
            pain_debounce_time: self.pain_debounce_time,
            saber_lock_time: self.saber_lock_time,
            grip_being_gripped: self.grip_being_gripped,
            force_powers_active: self.force_powers_active,
            saber_in_flight: self.saber_in_flight,
            saber_entity_num: self.saber_entity_num,
        }
    }

    fn combat(&self) -> EnemyCombat {
        EnemyCombat {
            origin: self.origin,
            health: self.health,
            weapon: i32::from(self.weapon),
            view_angles: self.view_angles,
            weapon_time: self.weapon_time,
            weapon_state: self.weapon_state,
            force_powers_active: self.force_powers_active,
            saber_lock_time: self.saber_lock_time,
            saber_in_flight: self.saber_in_flight,
            saber_entity_num: self.saber_entity_num,
            saber_entity_state: self.saber_entity_state,
            pain_debounce_time: self.pain_debounce_time,
            muzzle_point: self.muzzle_point,
            muzzle_point_old: self.muzzle_point_old,
        }
    }
}

/// An NPC's off-sounds as `Cmd_ToggleSaber_f` plays them (`saber[n].soundOff`, the
/// second only where that hand holds a saber).
pub(crate) fn saber_off_sounds(
    npc: &crate::npc_spawn::NpcActor,
) -> [crate::force_powers::SaberOffSound; 2] {
    use crate::force_powers::SaberOffSound;
    let [first, second] = &npc.definition.sabers;
    let sound = |index: u16| {
        if index == 0 {
            SaberOffSound::Silent
        } else {
            SaberOffSound::Index(index)
        }
    };
    [
        sound(first.sound_off),
        if second.model.is_empty() {
            SaberOffSound::Silent
        } else {
            sound(second.sound_off)
        },
    ]
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// Entity `number` as the Jedi AI reads a client: an NPC's own record, a player's
    /// through the host; `None` for anything that is no client.
    pub fn jedi_client(&self, number: u16) -> Option<JediClient> {
        let Some(at) = self.actor_at(number) else {
            return self.host.jedi_player(number);
        };
        let npc = &self.actors[at];
        let state = &npc.player;
        let blade = |saber: usize| {
            let blade = &npc.saber.sabers.sabers[saber].blades[0];
            JediBlade {
                point: blade.point,
                dir: blade.direction,
                point_old: blade.point_old,
                dir_old: blade.direction_old,
            }
        };
        Some(JediClient {
            number,
            origin: npc.current_origin,
            mins: npc.mins,
            maxs: npc.maxs,
            current_angles: npc.mind.current_angles,
            health: npc.health,
            enemy: npc.mind.enemy,
            pain_debounce_time: npc.mind.fight.pain_debounce_time,
            attack_debounce_time: npc.mind.attack_debounce_time,
            view_angles: state.view_angles(),
            velocity: state.velocity(),
            legs_anim: state.leg_animation(),
            ground_entity: state.ground_entity_num(),
            weapon: state.weapon(),
            weapon_time: state.weapon_time(),
            weapon_state: state.weapon_state(),
            saber_move: state.saber_move(),
            saber_lock_time: state.saber_lock_time(),
            sabers_off: crate::npc_saber::sabers_off(npc),
            saber_in_flight: state.saber_in_flight(),
            saber_entity_num: state.saber_entity_num(),
            saber_entity_state: npc.mind.saber_entity_state,
            force_powers_active: state.force_powers_active(),
            grip_being_gripped: npc.force.grip_being_gripped,
            muzzle_point: npc.mind.muzzle_point,
            muzzle_point_old: npc.mind.muzzle_point_old,
            blades: [blade(0), blade(1)],
        })
    }

    /// A player's client as the distance rules read it.
    pub fn jedi_player_foe(&self, number: u16) -> Option<JediFoe> {
        self.host.jedi_player(number).map(|client| client.foe())
    }

    /// Entity `enemy`'s client as the evasions read it.
    pub(crate) fn jedi_enemy_combat(&self, enemy: u16) -> Option<EnemyCombat> {
        self.jedi_client(enemy).map(|client| client.combat())
    }

    /// Blade `blade` of saber `saber` of `enemy`'s client.
    pub(crate) fn jedi_enemy_blade(&self, enemy: u16, saber: usize, _blade: usize) -> EnemyBlade {
        let blade = self
            .jedi_client(enemy)
            .map(|client| client.blades[saber.min(1)])
            .unwrap_or_default();
        EnemyBlade {
            muzzle_point: blade.point,
            muzzle_dir: blade.dir,
            muzzle_point_old: blade.point_old,
            muzzle_dir_old: blade.dir_old,
            length: 0.0,
        }
    }

    /// A client's `ps.fd.forcePowersActive` (0 for anything else).
    pub fn client_force_powers_active(&self, number: u16) -> u32 {
        self.jedi_client(number)
            .map_or(0, |client| client.force_powers_active)
    }

    /// `g_forceDodge.integer` (default 1).
    pub(crate) fn jedi_force_dodge(&self) -> i32 {
        1
    }

    /// `ps.fd.forceJumpCharge = 0`.
    pub fn clear_force_jump_charge(&mut self, me: usize) {
        self.actors[me].force.jump_charge = 0.0;
    }

    /// `ps.forceHandExtendTime = time`.
    pub fn set_force_hand_extend_time(&mut self, me: usize, time: i32) {
        self.actors[me].mind.knockdown.hand_extend_time = time;
    }

    /// `ps.saberEventFlags & SEF_INWATER`.
    pub fn saber_in_water(&self, me: usize) -> bool {
        self.actors[me].saber.event_flags & crate::saber_clash::sef::IN_WATER != 0
    }

    /// Entity `number`'s place, trajectory delta and weapon (an NPC's saber entity, a
    /// player's thrown saber, a missile); all zero for anything unknown.
    pub(crate) fn jedi_entity_motion(&self, number: u16) -> EntityMotion {
        self.npc_saber_motion(number)
            .or_else(|| self.host.entity_motion(number))
            .unwrap_or_default()
    }

    /// An NPC's saber entity's `r.currentOrigin` (where it flies or lies, out of the hand;
    /// its blade's tip, in it) and its wire state's `s.pos.trDelta` and `s.weapon`.
    fn npc_saber_motion(&self, number: u16) -> Option<EntityMotion> {
        let npc = self
            .actors
            .iter()
            .find(|npc| npc.saber_entity == Some(number))?;
        let origin = if npc.player.saber_in_flight() {
            npc.saber.flight.current
        } else {
            npc.saber.entity.origin
        };
        let state = self.host.entity_state(number)?;
        let field = |index: usize| state.raw_field(index).unwrap_or(0);
        Some(EntityMotion {
            origin,
            delta: ES_POS_DELTA.map(|index| f32::from_bits(field(index))),
            weapon: field(ES_WEAPON) as i32,
        })
    }

    /// A client's thrown saber: its entity's `r.currentOrigin` and `s.pos.trDelta`.
    pub fn thrown_saber(&self, owner: u16) -> Option<([f32; 3], [f32; 3])> {
        let saber = self
            .jedi_client(owner)
            .filter(|client| client.saber_in_flight)?
            .saber_entity_num;
        self.npc_saber_motion(saber)
            .or_else(|| self.host.entity_motion(saber))
            .map(|motion| (motion.origin, motion.delta))
    }

    /// Whether the NPC's saber, out of its hand, lies still (`s.pos.trType ==
    /// TR_STATIONARY`, `NPC_AI_Jedi.c:5741`).
    pub fn npc_saber_stationary(&self, me: usize) -> bool {
        const ES_POS_TYPE: usize = 23;
        const TR_STATIONARY: u32 = 0;
        let npc = &self.actors[me];
        npc.player.saber_in_flight()
            && npc
                .saber_entity
                .and_then(|number| self.host.entity_state(number))
                .is_some_and(|state| state.raw_field(ES_POS_TYPE).unwrap_or(0) == TR_STATIONARY)
    }

    /// `Jedi_TryJump(tempGoal)` for an entity spawned at `point`.
    pub fn jedi_try_jump_to_point(
        &mut self,
        me: usize,
        point: [f32; 3],
        command: &mut UserCommand,
    ) -> bool {
        let Some(number) = self.host.spawn_entity() else {
            return false;
        };
        let jumped = self.jedi_try_jump_to(
            me,
            crate::npc_jedi_jump::JumpGoal {
                number,
                origin: point,
                client_on_ground: None,
            },
            command,
        );
        self.host.free(number);
        jumped
    }

    /// `WP_GetVelocityForForceJump(NPC, jumpVel, &ucmd)` (`w_force.c:2231-2314`): the jump
    /// the charge would make, and its way (`FJ_*`); the charge's sound muted and the jump's
    /// heard, as the reference's does whether or not it jumps.
    pub fn velocity_for_force_jump(
        &mut self,
        me: usize,
        command: &mut UserCommand,
    ) -> ([f32; 3], i32) {
        let command = *command;
        self.with_forcer(me, |forcer| forcer.jump_velocity(&command))
            .map_or(([0.0; 3], 0), |(velocity, way)| (velocity, way as i32))
    }

    /// `WP_ActivateSaber` (`w_saber.c:267-296`): an NPC's hand taunt cut short; unless
    /// gripped, its saber lit, with each saber's ignition sound.
    pub fn activate_saber(&mut self, me: usize) {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        if npc.player.force_hand_extend() == HANDEXTEND_JEDITAUNT
            && npc.mind.knockdown.hand_extend_time - level_time > 200
        {
            npc.player.set_raw_field(PS_FORCE_HAND_EXTEND, 0);
            npc.mind.knockdown.hand_extend_time = 0;
        } else if npc.player.raw_field(PS_GRIP_CRIPPLE).unwrap_or(0) != 0 {
            return;
        }
        if npc.player.saber_holstered() == 0 {
            return;
        }
        npc.player.set_raw_field(PS_SABER_HOLSTERED, 0);
        let sounds = [
            npc.definition.sabers[0].sound_on,
            npc.definition.sabers[1].sound_on,
        ];
        for sound in sounds.into_iter().filter(|sound| *sound != 0) {
            self.saber_sound(me, sound);
        }
    }

    /// `WP_DeactivateSaber` (`w_saber.c:236-265`): a lit saber put away with each saber's
    /// sound (the second's only if it has a model). The blade's length is not cleared
    /// ("Doens't matter ATM").
    pub fn deactivate_saber(&mut self, me: usize, _clear_length: bool) {
        let npc = &mut self.actors[me];
        if npc.player.saber_holstered() != 0 {
            return;
        }
        npc.player.set_raw_field(PS_SABER_HOLSTERED, 2);
        let [first, second] = &npc.definition.sabers;
        let sounds = [
            first.sound_off,
            if second.model.is_empty() {
                0
            } else {
                second.sound_off
            },
        ];
        for sound in sounds.into_iter().filter(|sound| *sound != 0) {
            self.saber_sound(me, sound);
        }
    }

    /// `G_Sound(NPC, CHAN_WEAPON, sound)`.
    fn saber_sound(&mut self, me: usize, sound: u16) {
        let origin = self.actors[me].current_origin;
        self.host
            .raise(crate::weapon_fire::sound_event(origin, CHAN_WEAPON, sound));
    }

    /// `ForceSpeed(NPC, duration)`.
    pub fn force_speed(&mut self, me: usize, duration: i32) {
        self.with_forcer(me, |forcer| forcer.speed(duration));
    }

    /// `ForceLightning(NPC)`.
    pub fn force_lightning(&mut self, me: usize) {
        self.with_forcer(me, |forcer| forcer.lightning());
    }

    /// `ForceHeal(NPC)`.
    pub fn force_heal(&mut self, me: usize) {
        self.with_forcer(me, |forcer| forcer.heal());
    }

    /// `ForceRage(NPC)`.
    pub fn force_rage(&mut self, me: usize) {
        self.with_forcer(me, |forcer| forcer.guard(FP_RAGE));
    }

    /// `ForceProtect(NPC)`.
    pub fn force_protect(&mut self, me: usize) {
        self.with_forcer(me, |forcer| forcer.guard(FP_PROTECT));
    }

    /// `ForceAbsorb(NPC)`.
    pub fn force_absorb(&mut self, me: usize) {
        self.with_forcer(me, |forcer| forcer.guard(FP_ABSORB));
    }

    /// `ForceDrain(NPC)`.
    pub fn force_drain(&mut self, me: usize) {
        self.with_forcer(me, |forcer| forcer.force_drain());
    }

    /// `WP_ForcePowerUsable(NPC, power)`.
    pub fn force_power_usable(&mut self, me: usize, power: usize) -> bool {
        let (level_time, gametype) = (self.level_time, self.host.gametype());
        let npc = &self.actors[me];
        crate::force_powers::usable(
            &npc.player,
            &npc.force,
            npc.health,
            power,
            level_time,
            gametype,
        )
    }

    /// `WP_ForcePowerAvailable(NPC, power, overrideAmt)`.
    pub fn force_power_available(&self, me: usize, power: usize, override_amount: i32) -> bool {
        let npc = &self.actors[me];
        crate::force_powers::available(&npc.player, &npc.force, power, override_amount)
    }

    /// `WP_ForcePowerStop(NPC, power)`.
    pub fn force_power_stop(&mut self, me: usize, power: usize) {
        self.with_forcer(me, |forcer| forcer.stop(power));
    }
}
