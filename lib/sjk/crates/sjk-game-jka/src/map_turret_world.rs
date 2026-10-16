//! What the two map turrets share (`codemp/game/g_turret_G2.c`, `g_turret.c`): the level
//! as a turret reads it ([`Sighted`]), the world it asks for traces, indices, effects,
//! bolts and new missiles ([`TurretHost`]), and `G_Damage`'s part for a thing that is no
//! client ([`object_take`]), which is how a turret is hurt.
//!
//! A turret is an entity of the level's own: it keeps its wire state
//! ([`sjk_protocol::EntityState`]) whole, as the reference's `gentity_t::s`, and the host
//! sends it as it is. Entity numbers appear only where the reference's game profile has
//! them: the turret's own (its missiles' owner, the pass entity of its traces) and its
//! enemy's, both handed in by the host.

use crate::event_entity::EventEntity;
use crate::pmove::MovementTrace;
use crate::weapon_fire::Missile;
use sjk_protocol::EntityState;

/// `FRAMETIME`: a turret thinks every tenth of a second.
pub const FRAMETIME: i32 = 100;
/// `MASK_SHOT`: what a turret's sight and its bolts stop at.
pub const MASK_SHOT: u32 = 0x1301;
/// `CONTENTS_LIGHTSABER`, which a turret's bolts also meet.
pub const CONTENTS_LIGHTSABER: u32 = 0x4_0000;
/// `CONTENTS_BODY`.
pub const CONTENTS_BODY: u32 = 0x100;
/// `FL_NOTARGET`: nothing takes this entity for an enemy.
pub const FL_NOTARGET: u32 = 0x20;
/// `TEAM_SPECTATOR`.
pub const TEAM_SPECTATOR: i32 = 3;
/// `WP_BLASTER`, `WP_DEMP2`, `WP_EMPLACED_GUN`, `WP_TURRET`.
pub const WP_BLASTER: u32 = 5;
pub const WP_DEMP2: u32 = 9;
pub const WP_EMPLACED_GUN: u32 = 17;
pub const WP_TURRET: u32 = 18;
/// `EV_PLAY_EFFECT`, `EV_PLAY_EFFECT_ID`.
pub const EV_PLAY_EFFECT: u32 = 68;
pub const EV_PLAY_EFFECT_ID: u32 = 69;
/// `EFFECT_EXPLOSION_TURRET`, `EFFECT_SPARKS` (`bg_public.h`'s effect list).
pub const EFFECT_EXPLOSION_TURRET: u32 = 10;
pub const EFFECT_SPARKS: u32 = 11;
/// `CHAN_BODY`.
pub const CHAN_BODY: u32 = 6;
/// `START_DIS`: how far ahead of its bolt a turret's shot starts.
pub const START_DIS: f32 = 15.0;
/// `DAMAGE_NO_KNOCKBACK`, `DAMAGE_DEATH_KNOCKBACK`, `DAMAGE_HEAVY_WEAP_CLASS`,
/// `DAMAGE_NO_SELF_PROTECTION`.
pub const DAMAGE_NO_KNOCKBACK: u32 = 0x4;
pub const DAMAGE_DEATH_KNOCKBACK: u32 = 0x80;
pub const DAMAGE_HEAVY_WEAP_CLASS: u32 = 0x1000;
pub const DAMAGE_NO_SELF_PROTECTION: u32 = 0x4000;
/// `ET_GENERAL`: what a turret is on the wire.
pub const ET_GENERAL: u32 = 0;
/// `TR_STATIONARY`, `TR_LINEAR`, `TR_LINEAR_STOP`.
pub const TR_STATIONARY: u32 = 0;
pub const TR_LINEAR: u32 = 2;
pub const TR_LINEAR_STOP: u32 = 3;

/// The wire fields of `entityState_t` a turret writes (`msg.cpp`'s table).
pub(crate) mod es {
    pub use crate::npc_spawn::es::{
        ANGLES, APOS_BASE, APOS_TIME, APOS_TYPE, EFLAGS, G2_RADIUS, HEALTH, MAX_HEALTH,
        MODEL_GHOUL2, MODEL_INDEX, ORIGIN, POS_BASE, POS_TYPE, SHOULD_TARGET, TEAM_OWNER, TYPE,
        WEAPON,
    };
    pub const APOS_DELTA: [usize; 3] = [48, 44, 49];
    pub const APOS_DURATION: usize = 89;
    pub const LEGS_ANIM: usize = 16;
    pub const TORSO_ANIM: usize = 17;
    pub const GENERIC_ENEMY_INDEX: usize = 18;
    pub const LOOP_SOUND: usize = 55;
    pub const MODEL_INDEX2: usize = 41;
    pub const TORSO_FLIP: usize = 50;
    pub const OTHER_ENTITY_2: usize = 39;
    pub const OWNER: usize = 40;
    pub const EMPLACED_OWNER: usize = 47;
    pub const MODEL_SCALE: usize = 76;
    pub const FRAME: usize = 83;
    pub const EFLAGS2: usize = 96;
}

/// Something in the level a turret regards, as it reads it: a player, an NPC, a breakable
/// brush, another turret. The host lists them in entity order (`G_RadiusList` walks
/// `EntitiesInBox`, which the reference's fake engine answers in that order).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sighted {
    /// Its entity number.
    pub number: u16,
    /// `ent->client`: a player or an NPC.
    pub client: bool,
    /// `takedamage`, `health`, `FL_NOTARGET`.
    pub takes_damage: bool,
    pub health: i32,
    pub no_target: bool,
    /// `FL_BBRUSH`: a breakable brush, which a `misc_turretG2` shoots too.
    pub breakable: bool,
    /// `sess.sessionTeam` (a spectator's is [`TEAM_SPECTATOR`]) and `tempSpectate` of a
    /// client; `teamnodmg` of anything else.
    pub session_team: i32,
    pub temp_spectate_until: i32,
    pub team_no_damage: i32,
    /// `r.currentOrigin`, and a client's eye (`renderInfo.eyePoint`).
    pub origin: [f32; 3],
    pub eye: [f32; 3],
    /// `r.absmin`, `r.absmax`, and the top of its box (`r.maxs[2]`).
    pub bounds: ([f32; 3], [f32; 3]),
    pub top: f32,
    /// `ps.velocity` of a client, `s.pos.trDelta` of anything else: the lead.
    pub velocity: [f32; 3],
    /// An NPC of type `atst_vehicle`, which a `misc_turret` prefers; any walker vehicle,
    /// which it aims 32 units higher at.
    pub atst: bool,
    pub walker: bool,
}

/// Which of a turret's bolts (`genericValue11`, `genericValue12`).
pub type BoltIndex = i32;

/// A shot a turret fired: its missile, the entity's `dflags` (which its hit does not pass
/// on: `G_MissileImpact` damages with its own flags), and whether it has no `parent` —
/// a `misc_turretG2`'s own bolt, whose splash is nobody's.
#[derive(Clone, Debug, PartialEq)]
pub struct Shot {
    pub missile: Missile,
    pub dflags: u32,
    pub parentless: bool,
}

/// What a turret asks of the level beyond itself.
pub trait TurretHost {
    /// The game's generator (`Q_flrand`, `flrand`, `Q_irand`).
    fn rng(&mut self) -> &mut crate::player_death::Rng;
    /// `G_ModelIndex`, `G_SoundIndex`, `G_EffectIndex`, `G_BoneIndex`, `G_IconIndex`.
    fn model_index(&mut self, name: &[u8]) -> u16;
    fn sound_index(&mut self, name: &[u8]) -> u16;
    fn effect_index(&mut self, name: &[u8]) -> u16;
    fn bone_index(&mut self, name: &[u8]) -> u16;
    fn icon_index(&mut self, name: &[u8]) -> u16;
    /// `RegisterItem(BG_FindItemForWeapon(weapon))`: the weapon's item precached.
    fn register_weapon(&mut self, weapon: u32);
    /// `trap->Trace(start, NULL, NULL, end, pass, mask)`: a point trace.
    fn trace(&mut self, start: [f32; 3], end: [f32; 3], pass: u16, mask: u32) -> MovementTrace;
    /// `trap->InPVS`.
    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool;
    /// `trap->PointContents(point, pass)`.
    fn point_contents(&mut self, point: [f32; 3], pass: u16) -> u32;
    /// `G2API_InitGhoul2Model` on turret `me`'s server instance: the model it wears now
    /// (a respawn wears it again).
    fn init_model(&mut self, me: u16, model: &[u8]);
    /// `G2API_RemoveGhoul2Model` and `G_KillG2Queue`: the instance gone, and the clients
    /// told to drop theirs.
    fn remove_model(&mut self, me: u16);
    /// `G2API_AddBolt` on turret `me`'s model.
    fn add_bolt(&mut self, me: u16, name: &[u8]) -> BoltIndex;
    /// `G2API_GetBoltMatrix` of turret `me`'s bolt at `level.time`, placed at `origin`
    /// turned by `angles` and scaled by `scale`: the 3x4 matrix (`BG_GiveMeVectorFromMatrix`
    /// reads its columns).
    fn bolt_matrix(
        &mut self,
        me: u16,
        bolt: BoltIndex,
        angles: [f32; 3],
        origin: [f32; 3],
        scale: [f32; 3],
    ) -> [[f32; 4]; 3];
    /// `G2API_SetBoneAngles` on turret `me`'s server instance (`BONE_ANGLES_POSTMULT`,
    /// +Y up, -Z right, -X forward), which moves its bolts.
    fn set_bone_angles(&mut self, me: u16, bone: &[u8], angles: [f32; 3]);
    /// `G2API_SetBoneAnim(model_root, start, end, OVERRIDE_FREEZE|BLEND, 1.0)` on the
    /// turbolaser's server instance.
    fn set_bone_anim(&mut self, me: u16, start: i32, end: i32);
    /// `G_TempEntity`: an event entity, numbered now.
    fn raise(&mut self, event: EventEntity);
    /// `G_Spawn` for a turret's shot, numbered now.
    fn launch(&mut self, shot: Shot);
    /// `G_UseTargets2(me, activator, name)`: everything named `name` used.
    fn use_targets(&mut self, name: &str, me: u16, activator: Option<u16>);
    /// `G_RadiusDamage(origin, attacker, damage, radius, ignore, NULL, mod)`.
    fn radius_damage(
        &mut self,
        origin: [f32; 3],
        attacker: Option<u16>,
        damage: f32,
        radius: f32,
        ignore: Option<u16>,
        means: u32,
    );
}

/// The attacker `G_Damage` reads when a turret is struck.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlowAttacker {
    /// Its entity number (`ENTITYNUM_WORLD` for the world).
    pub number: u16,
    /// A client, and one that is a player (`s.eType == ET_PLAYER`).
    pub client: bool,
    pub player: bool,
    /// `ps.stats[STAT_MAX_HEALTH]`: the handicap.
    pub max_health: i32,
    /// `sess.sessionTeam` of a client, `teamnodmg` of anything else.
    pub team: i32,
    /// `ps.weapon`: a DEMP2 hit stuns a turret.
    pub weapon: u32,
    /// `activator->client->sess.sessionTeam` of a thing whose activator is a player (an
    /// emplaced gun's gunner).
    pub activator_team: Option<i32>,
}

/// A blow on a turret, as `G_Damage` is asked for it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectBlow {
    /// `None` for the world.
    pub attacker: Option<BlowAttacker>,
    pub damage: i32,
    pub flags: u32,
    pub means: u32,
    /// Siege's rules: nothing is hurt before the round begins, and `teamnodmg` holds
    /// (unless `g_ff_objectives`).
    pub siege: bool,
    pub siege_round_begun: bool,
    pub friendly_fire_objectives: bool,
}

/// `G_Damage` (`g_combat.c:4425-4960`) up to the health taken, for entity `me` that is no
/// client: `None` where it returns before (siege not begun, no damage taken, `teamnodmg`),
/// else what comes off the health — the handicap, half on oneself, at least one.
pub fn object_take(
    me: u16,
    takes_damage: bool,
    team_no_damage: i32,
    blow: &ObjectBlow,
) -> Option<i32> {
    if blow.siege && !blow.siege_round_begun {
        return None;
    }
    if !takes_damage {
        return None;
    }
    let mut damage = blow.damage;
    if let Some(attacker) = blow.attacker.filter(|attacker| {
        attacker.client && attacker.number != me && attacker.player && !blow.siege
    }) {
        damage = damage * attacker.max_health / 100;
    }
    // "check for teamnodmg": in siege only, and not for a client's own team's objects.
    if let Some(attacker) = blow.attacker
        && blow.siege
        && !blow.friendly_fire_objectives
        && team_no_damage != 0
        && attacker.team == team_no_damage
        && (attacker.client
            || attacker
                .activator_team
                .is_none_or(|team| team == team_no_damage))
    {
        return None;
    }
    if blow.attacker.is_some_and(|attacker| attacker.number == me)
        && blow.flags & DAMAGE_NO_SELF_PROTECTION == 0
    {
        // `damage *= 1.5` / `0.5` on an int: the double product, truncated.
        damage = (f64::from(damage) * if blow.siege { 1.5 } else { 0.5 }) as i32;
    }
    Some(damage.max(1))
}

/// `G_ScaleNetHealth` (`g_utils.c:1116-1146`): the health a client's bar shows, and its
/// maximum — divided by a hundred from a thousand up, never below nothing, never nothing
/// for a thing still standing.
pub fn net_health(health: i32, max_health: i32) -> (i32, i32) {
    if max_health < 1_000 {
        return (health.max(0), max_health);
    }
    let mut shown = (health / 100).max(0);
    if health > 0 && shown <= 0 {
        shown = 1;
    }
    (shown, max_health / 100)
}

/// [`net_health`] written onto a wire state (`s.health`, `s.maxhealth`).
pub fn publish_net_health(state: &mut EntityState, health: i32, max_health: i32) {
    let (shown, max) = net_health(health, max_health);
    state.set_raw_field(es::HEALTH, shown as u32);
    state.set_raw_field(es::MAX_HEALTH, max as u32);
}

/// `G_SetEnemy` for a thing that is no NPC (`NPC_combat.c:373-393`): the enemy, where it is
/// in the level and not `FL_NOTARGET`; else the one it had.
pub fn set_enemy(current: Option<u16>, candidate: Option<&Sighted>) -> Option<u16> {
    match candidate {
        Some(enemy) if !enemy.no_target => Some(enemy.number),
        _ => current,
    }
}

/// The target numbered `number`, if the level lists it (an entity it does not list is
/// read as no longer in use).
pub fn find(targets: &[Sighted], number: Option<u16>) -> Option<&Sighted> {
    let number = number?;
    targets.iter().find(|target| target.number == number)
}

/// `G_PlayEffect(effect, origin, angles)` (`g_utils.c:1254-1264`).
pub fn play_effect(host: &mut dyn TurretHost, effect: u32, origin: [f32; 3], angles: [f32; 3]) {
    host.raise(effect_event(EV_PLAY_EFFECT, effect, origin, angles));
}

/// `G_PlayEffectID(effect, origin, angles)` (`g_utils.c:1271-1288`): no angles play along +y.
pub fn play_effect_id(host: &mut dyn TurretHost, effect: u32, origin: [f32; 3], angles: [f32; 3]) {
    let angles = if angles == [0.0; 3] {
        [0.0, 1.0, 0.0]
    } else {
        angles
    };
    host.raise(effect_event(EV_PLAY_EFFECT_ID, effect, origin, angles));
}

/// A temporary entity raising `event` (an effect by number) at `origin` along `angles`.
pub(crate) fn effect_event(
    event: u32,
    effect: u32,
    origin: [f32; 3],
    angles: [f32; 3],
) -> EventEntity {
    let mut raised = EventEntity {
        event,
        parameter: effect,
        origin,
        client: None,
        broadcast: false,
        extra: [(0, 0); 12],
    };
    for axis in 0..3 {
        raised.extra[axis] = (es::ORIGIN[axis], origin[axis].to_bits());
        raised.extra[3 + axis] = (es::ANGLES[axis], angles[axis].to_bits());
    }
    raised
}

/// `G_Sound(ent, CHAN_BODY, G_SoundIndex(name))` from `origin`.
pub fn body_sound(host: &mut dyn TurretHost, origin: [f32; 3], name: &[u8]) {
    let index = host.sound_index(name);
    host.raise(crate::weapon_fire::sound_event(origin, CHAN_BODY, index));
}

/// The spawn keys as `G_ParseField` and `G_SpawnInt`/`G_SpawnFloat`/`G_SpawnString` read
/// them: `atoi`, `atof`, a default where the key is absent.
pub struct Keys<'e>(pub &'e sjk_entity::Entity);

impl Keys<'_> {
    /// An `F_INT` key.
    pub fn int(&self, key: &str, default: i32) -> i32 {
        self.0
            .get(key)
            .map_or(default, |text| crate::userinfo::atoi(text.as_bytes()))
    }

    /// An `F_FLOAT` key.
    pub fn float(&self, key: &str, default: f32) -> f32 {
        self.0
            .get(key)
            .map_or(default, |text| crate::text_parse::atof(text.as_bytes()))
    }

    /// An `F_STRING` key, empty where absent.
    pub fn text(&self, key: &str) -> String {
        self.0.get(key).unwrap_or_default().to_owned()
    }

    /// `origin` (`F_VECTOR`).
    pub fn origin(&self) -> [f32; 3] {
        crate::fx_runner::vector(self.0, "origin")
    }

    /// `s.angles` from `angles` or `angle`.
    pub fn angles(&self) -> [f32; 3] {
        crate::fx_runner::spawn_angles(self.0)
    }

    /// `teamnodmg`, or the `team` key read as a number where it gave none
    /// (`finish_spawning_turretG2`, `turret_base_spawn_top`).
    pub fn team_no_damage(&self) -> i32 {
        match self.int("teamnodmg", 0) {
            0 if !self.text("team").is_empty() => {
                crate::userinfo::atoi(self.text("team").as_bytes())
            }
            given => given,
        }
    }
}

/// `G_SetAngles` and `G_SetOrigin` on a wire state: `s.angles` and `s.apos.trBase` the
/// angles, a stationary `s.pos` at the origin.
pub fn place(state: &mut EntityState, origin: [f32; 3], angles: [f32; 3]) {
    for axis in 0..3 {
        state.set_raw_field(es::ANGLES[axis], angles[axis].to_bits());
        state.set_raw_field(es::APOS_BASE[axis], angles[axis].to_bits());
        state.set_raw_field(es::POS_BASE[axis], origin[axis].to_bits());
    }
    state.set_raw_field(es::POS_TYPE, TR_STATIONARY);
}

/// `s.apos` as `BG_EvaluateTrajectory` reads it at `level_time`.
pub fn evaluate_angles(state: &EntityState, level_time: i32) -> [f32; 3] {
    let read = |index: usize| f32::from_bits(state.raw_field(index).unwrap_or(0));
    let base = es::APOS_BASE.map(read);
    let delta = es::APOS_DELTA.map(read);
    let kind = state.raw_field(es::APOS_TYPE).unwrap_or(0) as u8;
    let start = state.raw_field(es::APOS_TIME).unwrap_or(0) as i32;
    let duration = state.raw_field(es::APOS_DURATION).unwrap_or(0) as i32;
    crate::trajectory::legacy_evaluate_trajectory_angles(
        base, delta, kind, start, duration, level_time,
    )
}

/// `s.apos`: its base, its delta, its type, time and duration written.
pub fn set_apos(
    state: &mut EntityState,
    base: [f32; 3],
    delta: [f32; 3],
    kind: u32,
    time: i32,
    duration: i32,
) {
    for axis in 0..3 {
        state.set_raw_field(es::APOS_BASE[axis], base[axis].to_bits());
        state.set_raw_field(es::APOS_DELTA[axis], delta[axis].to_bits());
    }
    state.set_raw_field(es::APOS_TYPE, kind);
    state.set_raw_field(es::APOS_TIME, time as u32);
    state.set_raw_field(es::APOS_DURATION, duration as u32);
}

/// `VectorCopy(r.currentAngles, s.apos.trBase); VectorClear(s.apos.trDelta)`: the turn
/// stopped where it stands (its type and times left as they were).
pub fn stop_apos(state: &mut EntityState, angles: [f32; 3]) {
    for axis in 0..3 {
        state.set_raw_field(es::APOS_BASE[axis], angles[axis].to_bits());
        state.set_raw_field(es::APOS_DELTA[axis], 0);
    }
}

pub(crate) fn sub(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}

pub(crate) fn length_squared(vector: [f32; 3]) -> f32 {
    vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]
}

/// `BG_GiveMeVectorFromMatrix`'s `ORIGIN` and `POSITIVE_X` of a bolt matrix.
pub fn matrix_origin_and_x(matrix: [[f32; 4]; 3]) -> ([f32; 3], [f32; 3]) {
    (
        std::array::from_fn(|row| matrix[row][3]),
        std::array::from_fn(|row| matrix[row][0]),
    )
}
