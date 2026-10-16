//! Which behaviour an NPC runs (`NPC_RunBehavior` and the `NPC_BehaviorSet_*` tables of
//! `codemp/game/NPC.c:876-1515`) and the behaviours of the default set a thinking NPC with
//! no class of its own reaches: `NPC_BSDefault` (`NPC_AI_Default.c:720-965`), `NPC_BSWait`
//! and `NPC_BSCinematic` (`NPC_behavior.c:231-267`).
//!
//! The table is the reference's whole: by class, weapon, team and behaviour state it names
//! the function the reference would run ([`Behavior`]). The stormtroopers' AI runs
//! ([`crate::npc_st`], and `NPC_StartFlee`). The other classes' own AI — Jedi, the rancor,
//! the wampa, droids and the rest — and the default set's other states (following,
//! searching, wandering, fleeing, ...) are the NPC plan's later steps: [`Behavior::Stub`]
//! names them and the host is told ([`NpcHost::stub`]); the NPC does nothing for that
//! think, and the rest of its think runs as the reference's.

use crate::npc_enemy::{NPCTEAM_ENEMY, NPCTEAM_NEUTRAL};
use crate::npc_spawn::NpcHost;
use crate::npc_world::{NpcWorld, SCF_FORCED_MARCH};

/// `bState_t` (`g_public.h:119-141`).
pub mod bstate {
    pub const DEFAULT: i32 = 0;
    pub const ADVANCE_FIGHT: i32 = 1;
    pub const SLEEP: i32 = 2;
    pub const FOLLOW_LEADER: i32 = 3;
    pub const JUMP: i32 = 4;
    pub const SEARCH: i32 = 5;
    pub const WANDER: i32 = 6;
    pub const NOCLIP: i32 = 7;
    pub const REMOVE: i32 = 8;
    pub const CINEMATIC: i32 = 9;
    pub const WAIT: i32 = 10;
    pub const STAND_GUARD: i32 = 11;
    pub const PATROL: i32 = 12;
    pub const INVESTIGATE: i32 = 13;
    pub const STAND_AND_SHOOT: i32 = 14;
    pub const HUNT_AND_KILL: i32 = 15;
    pub const FLEE: i32 = 16;
}

/// `class_t`s the table names.
mod class {
    pub const ATST: i32 = 1;
    pub const HOWLER: i32 = 13;
    pub const INTERROGATOR: i32 = 16;
    pub const JEDI: i32 = 18;
    pub const MARK1: i32 = 23;
    pub const MARK2: i32 = 24;
    pub const GALAKMECH: i32 = 25;
    pub const MINEMONSTER: i32 = 26;
    pub const PROBE: i32 = 32;
    pub const PROTOCOL: i32 = 33;
    pub const REBORN: i32 = 37;
    pub const REMOTE: i32 = 39;
    pub const SEEKER: i32 = 41;
    pub const SENTRY: i32 = 42;
    pub const UGNAUGHT: i32 = 49;
    pub const JAWA: i32 = 50;
    pub const BOBAFETT: i32 = 52;
    pub const VEHICLE: i32 = 53;
    pub const RANCOR: i32 = 54;
    pub const WAMPA: i32 = 55;
}

/// `weapon_t`s the table names.
const WP_SABER: u8 = 3;
const WP_MELEE: u8 = 2;
const WP_DISRUPTOR: u8 = 6;
const WP_STUN_BATON: u8 = 1;
const WP_THERMAL: u8 = 12;
const WP_EMPLACED_GUN: u8 = 17;
/// `SCF_ALT_FIRE`, `SCF_FIRE_WEAPON`, `SCF_LOOK_FOR_ENEMIES`, `SCF_IGNORE_ALERTS`,
/// `SCF_RUNNING`, `SCF_WALKING`.
const SCF_ALT_FIRE: u32 = 0x40;
const SCF_LOOK_FOR_ENEMIES: u32 = 0x800;
const SCF_IGNORE_ALERTS: u32 = 0x2000;
const SCF_FIRE_WEAPON: u32 = 0x4_0000;
/// `SCF_RUNNING`, `SCF_WALKING`, `SCF_FACE_MOVE_DIR`; `BUTTON_WALKING`.
const SCF_RUNNING: u32 = 0x20;
const SCF_WALKING: u32 = 0x2;
const SCF_FACE_MOVE_DIR: u32 = 0x1000;
const BUTTON_WALKING: u16 = 16;
/// `EF2_FLYING`.
const EF2_FLYING: u32 = 1 << 4;
/// `TORSO_SURRENDER_START`; `SETANIM_TORSO`, `SETANIM_FLAG_HOLD`.
const TORSO_SURRENDER_START: u16 = 1_409;

/// The function `NPC_RunBehavior` runs for an NPC this think.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Behavior {
    /// `NPC_BSDefault`.
    Default,
    /// `NPC_BSWait`: face the angles it has.
    Wait,
    /// `NPC_BSCinematic`: face them, and go where a script says.
    Cinematic,
    /// A vehicle's: face its spawn angles (`NPC_UpdateAngles`).
    Vehicle,
    /// The function of a later step, by its name in the reference.
    Stub(&'static str),
    /// `NPC_BehaviorSet_Stormtrooper` (`NPC.c:1110-1134`), after `NPC_CheckSurrender`
    /// (which never surrenders: its only `return qtrue`s are commented out,
    /// `NPC_behavior.c:1363-1464`).
    Stormtrooper(StState),
    /// `NPC_StartFlee` from its enemy: an unarmed NPC of the enemy team in a fight.
    StartFlee,
}

/// The stormtroopers' states (`NPC_BSST_Default`, `NPC_BSST_Investigate`,
/// `NPC_BSST_Sleep`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StState {
    Default,
    Investigate,
    Sleep,
}

impl StState {
    /// Its function's name in the reference.
    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "NPC_BSST_Default",
            Self::Investigate => "NPC_BSST_Investigate",
            Self::Sleep => "NPC_BSST_Sleep",
        }
    }
}

/// `NPC_BehaviorSet_Default` (`NPC.c:907-947`).
fn default_set(state: i32) -> Behavior {
    use bstate::*;
    match state {
        ADVANCE_FIGHT => Behavior::Stub("NPC_BSAdvanceFight"),
        SLEEP => Behavior::Stub("NPC_BSSleep"),
        FOLLOW_LEADER => Behavior::Stub("NPC_BSFollowLeader"),
        JUMP => Behavior::Stub("NPC_BSJump"),
        REMOVE => Behavior::Stub("NPC_BSRemove"),
        SEARCH => Behavior::Stub("NPC_BSSearch"),
        NOCLIP => Behavior::Stub("NPC_BSNoClip"),
        WANDER => Behavior::Stub("NPC_BSWander"),
        FLEE => Behavior::Stub("NPC_BSFlee"),
        WAIT => Behavior::Wait,
        CINEMATIC => Behavior::Cinematic,
        _ => Behavior::Default,
    }
}

/// `NPC_BehaviorSet_Charmed` (`NPC.c:876-901`).
fn charmed_set(state: i32) -> Behavior {
    use bstate::*;
    match state {
        FOLLOW_LEADER => Behavior::Stub("NPC_BSFollowLeader"),
        REMOVE => Behavior::Stub("NPC_BSRemove"),
        SEARCH => Behavior::Stub("NPC_BSSearch"),
        WANDER => Behavior::Stub("NPC_BSWander"),
        FLEE => Behavior::Stub("NPC_BSFlee"),
        _ => Behavior::Default,
    }
}

/// A class set whose default states (`BS_DEFAULT`, and those listed) run `name`, and whose
/// other states fall back to the default set (`NPC.c:956-1315`).
fn class_set(state: i32, also: &[i32], name: &'static str) -> Behavior {
    if state == bstate::DEFAULT || also.contains(&state) {
        Behavior::Stub(name)
    } else {
        default_set(state)
    }
}

/// The states the fighting classes treat as their default.
const FIGHTING: [i32; 4] = [
    bstate::STAND_GUARD,
    bstate::PATROL,
    bstate::STAND_AND_SHOOT,
    bstate::HUNT_AND_KILL,
];
/// The states the droids and the first Mark treat as their default.
const GUARDING: [i32; 2] = [bstate::STAND_GUARD, bstate::PATROL];
/// The states the AT-ST and the second Mark treat as their default.
const WALKING: [i32; 3] = [
    bstate::PATROL,
    bstate::STAND_AND_SHOOT,
    bstate::HUNT_AND_KILL,
];

/// `NPC_BehaviorSet_Jedi` (`NPC.c:1142-1163`).
fn jedi_set(state: i32) -> Behavior {
    if state == bstate::FOLLOW_LEADER {
        Behavior::Stub("NPC_BSJedi_FollowLeader")
    } else {
        class_set(state, &FIGHTING, "NPC_BSJedi_Default")
    }
}

/// `NPC_BehaviorSet_Seeker` (`NPC.c:1007-1022`).
fn seeker_set(state: i32) -> Behavior {
    class_set(state, &FIGHTING, "NPC_BSSeeker_Default")
}

/// What the NPC's class, weapon, team and flags make it (`NPC_RunBehavior`,
/// `NPC.c:1322-1515`), the vehicle's case aside.
#[derive(Clone, Copy, Debug)]
pub struct Traits {
    pub class: i32,
    pub weapon: u8,
    pub team: i32,
    pub flags2: u32,
    pub script_flags: u32,
    pub charmed: bool,
    /// `Jedi_CultistDestroyer`: a reborn called `cultist_destroyer` with the fists.
    pub cultist_destroyer: bool,
    /// Has an enemy (for the unarmed enemy's flight).
    pub has_enemy: bool,
}

/// `NPC_RunBehavior`'s table: which function runs for `state`.
pub fn run_behavior(traits: Traits, state: i32) -> Behavior {
    use bstate::*;
    let Traits {
        class,
        weapon,
        team,
        ..
    } = traits;
    if state == CINEMATIC {
        return Behavior::Cinematic;
    }
    if weapon == WP_EMPLACED_GUN {
        // `NPC_CheckCharmed` follows, which does nothing for an NPC never charmed.
        return Behavior::Stub("NPC_BSEmplaced");
    }
    if class == class::JEDI || class == class::REBORN || weapon == WP_SABER {
        return jedi_set(state);
    }
    match class {
        class::WAMPA => return Behavior::Stub("NPC_BSWampa_Default"),
        class::RANCOR => return class_set(state, &FIGHTING, "NPC_BSRancor_Default"),
        class::REMOTE => return Behavior::Stub("NPC_BSRemote_Default"),
        class::SEEKER => return seeker_set(state),
        class::BOBAFETT if traits.flags2 & EF2_FLYING != 0 => return seeker_set(state),
        class::BOBAFETT => return jedi_set(state),
        _ => {}
    }
    if traits.cultist_destroyer {
        return Behavior::Stub("NPC_BSJedi_Default");
    }
    if traits.script_flags & SCF_FORCED_MARCH != 0 {
        return Behavior::Default;
    }
    match team {
        NPCTEAM_ENEMY => enemy_team(traits, state),
        NPCTEAM_NEUTRAL => match class {
            class::PROTOCOL | class::UGNAUGHT | class::JAWA => default_set(state),
            class::VEHICLE => Behavior::Vehicle,
            _ => class_set(state, &GUARDING, "NPC_BSDroid_Default"),
        },
        _ if class == class::SEEKER => seeker_set(state),
        _ if traits.charmed => charmed_set(state),
        _ => default_set(state),
    }
}

/// `NPC_RunBehavior`'s enemy team (`NPC.c:1400-1470`): the enemy droids and monsters by
/// class, the unarmed in a fight fleeing, the saber, the sniper, the grenadier, and the
/// stormtroopers.
fn enemy_team(traits: Traits, state: i32) -> Behavior {
    use bstate::*;
    match traits.class {
        class::ATST => return class_set(state, &WALKING, "NPC_BSATST_Default"),
        class::PROBE => return class_set(state, &FIGHTING, "NPC_BSImperialProbe_Default"),
        class::REMOTE => return Behavior::Stub("NPC_BSRemote_Default"),
        class::SENTRY => return class_set(state, &FIGHTING, "NPC_BSSentry_Default"),
        class::INTERROGATOR => return class_set(state, &FIGHTING, "NPC_BSInterrogator_Default"),
        class::MINEMONSTER => return class_set(state, &FIGHTING, "NPC_BSMineMonster_Default"),
        class::HOWLER => return class_set(state, &FIGHTING, "NPC_BSHowler_Default"),
        class::MARK1 => return class_set(state, &GUARDING, "NPC_BSMark1_Default"),
        class::MARK2 => return class_set(state, &WALKING, "NPC_BSMark2_Default"),
        class::GALAKMECH => return Behavior::Stub("NPC_BSGM_Default"),
        _ => {}
    }
    if traits.has_enemy && traits.weapon == 0 && state != HUNT_AND_KILL {
        return if state != FLEE {
            Behavior::StartFlee
        } else {
            Behavior::Stub("NPC_BSFlee")
        };
    }
    if traits.weapon == WP_SABER {
        return default_set(state);
    }
    if traits.weapon == WP_DISRUPTOR && traits.script_flags & SCF_ALT_FIRE != 0 {
        return class_set(state, &FIGHTING, "NPC_BSSniper_Default");
    }
    if traits.weapon == WP_THERMAL || traits.weapon == WP_STUN_BATON {
        return class_set(state, &FIGHTING, "NPC_BSGrenadier_Default");
    }
    match state {
        STAND_GUARD | PATROL | STAND_AND_SHOOT | HUNT_AND_KILL | DEFAULT => {
            Behavior::Stormtrooper(StState::Default)
        }
        INVESTIGATE => Behavior::Stormtrooper(StState::Investigate),
        SLEEP => Behavior::Stormtrooper(StState::Sleep),
        _ => default_set(state),
    }
}

/// `Jedi_CultistDestroyer` (`NPC_AI_Jedi.c:187-199`).
pub fn cultist_destroyer(class: i32, weapon: u8, npc_type: &[u8]) -> bool {
    class == class::REBORN
        && weapon == WP_MELEE
        && npc_type.eq_ignore_ascii_case(b"cultist_destroyer")
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// Runs `behavior` for the NPC at `me`: the default set's own, or the stub of a later
    /// step's, which the host is told of. `command` is the think's (`NPCS.ucmd`).
    pub fn run(&mut self, me: usize, behavior: Behavior, command: &mut sjk_protocol::UserCommand) {
        if self.level.native_sand_creatures
            && crate::npc_sand_creature::is_sand_creature(&self.actors[me])
            && !matches!(behavior, Behavior::Wait | Behavior::Cinematic)
        {
            self.actors[me].mind.unported = None;
            self.bs_sand_creature(me, command);
            return;
        }
        self.actors[me].mind.unported = match behavior {
            Behavior::Stub(name) => Some(name),
            _ => None,
        };
        match behavior {
            Behavior::Default => self.bs_default(me, command),
            Behavior::Wait | Behavior::Cinematic | Behavior::Vehicle => {
                // `NPC_BSCinematic` without a script: no weapon fired, no goal, nobody
                // watched — it only faces its angles, as `NPC_BSWait` does.
                if behavior == Behavior::Cinematic
                    && self.actors[me].script_flags & SCF_FIRE_WEAPON != 0
                {
                    self.weapon_think(me, command);
                }
                self.update_angles(me, true, true, command);
            }
            // The Jedi's AI ([`crate::npc_jedi`]), but for a replay whose driver stood it in.
            Behavior::Stub("NPC_BSJedi_Default") if !self.level.jedi_ai_stood_in() => {
                self.actors[me].mind.unported = None;
                self.bs_jedi_default(me, command);
            }
            Behavior::Stub("NPC_BSJedi_FollowLeader") if !self.level.jedi_ai_stood_in() => {
                self.actors[me].mind.unported = None;
                self.bs_jedi_follow_leader(me, command);
            }
            // The monsters' and droids' own ([`crate::npc_creature`]).
            Behavior::Stub(name) if self.run_creature(me, name, command) => {
                self.actors[me].mind.unported = None
            }
            // The default set's own states ([`crate::npc_states`]).
            Behavior::Stub(name) if self.run_state(me, name, command) => {
                self.actors[me].mind.unported = None
            }
            // The sniper's and the grenadier's ([`crate::npc_sniper`], [`crate::npc_grenadier`]).
            Behavior::Stub(name) if self.run_soldier(me, name, command) => {
                self.actors[me].mind.unported = None
            }
            Behavior::Stub(name) => {
                self.host.stub(self.actors[me].number, name);
            }
            Behavior::Stormtrooper(state) if self.level.stub_class_ai => {
                let number = self.actors[me].number;
                if self.actors[me].mind.enemy.is_some() {
                    self.host.stub(number, "NPC_CheckSurrender");
                }
                self.host.stub(number, state.name());
            }
            Behavior::Stormtrooper(StState::Default) => self.bs_st_default(me, command),
            Behavior::Stormtrooper(StState::Investigate) => self.bs_st_investigate(me, command),
            Behavior::Stormtrooper(StState::Sleep) => self.bs_st_sleep(me),
            Behavior::StartFlee if self.level.stub_class_ai => {
                self.host.stub(self.actors[me].number, "NPC_StartFlee")
            }
            Behavior::StartFlee => {
                let enemy = self.actors[me]
                    .mind
                    .enemy
                    .and_then(|enemy| self.body(enemy));
                self.start_flee(
                    me,
                    enemy.map(|enemy| enemy.number),
                    enemy.map_or([0.0; 3], |enemy| enemy.origin),
                    crate::npc_senses::AEL_DANGER + 1,
                    5_000,
                    10_000,
                );
            }
        }
    }

    /// `NPC_BSDefault` (`NPC_AI_Default.c:720-965`): look for an enemy (by sight, and by an
    /// alert an enemy made); with one, the stormtroopers' attack ([`crate::npc_st_attack`]);
    /// with a goal, go there; then face the desired angles.
    fn bs_default(&mut self, me: usize, command: &mut sjk_protocol::UserCommand) {
        let flags = self.actors[me].script_flags;
        if flags & SCF_FIRE_WEAPON != 0 {
            self.weapon_think(me, command);
        }
        if flags & SCF_FORCED_MARCH != 0
            && self.actors[me].player.torso_animation() != TORSO_SURRENDER_START
        {
            self.set_animation(
                me,
                crate::pmove_anim::SETANIM_TORSO,
                TORSO_SURRENDER_START,
                crate::pmove_anim::SETANIM_FLAG_HOLD,
            );
        }
        self.check_enemy(me, flags & SCF_LOOK_FOR_ENEMIES != 0, false, true);
        if self.actors[me].mind.enemy.is_none() && flags & SCF_IGNORE_ALERTS == 0 {
            self.notice_alerts(me);
        }
        if self.actors[me].mind.enemy.is_some() && flags & SCF_FORCED_MARCH == 0 {
            // `NPC_CheckGetNewWeapon` ([`crate::npc_weapon_pickup`]), and a leader's goal
            // given up (`NPC_AI_Default.c:778-784`).
            self.check_get_new_weapon_or_stub(me);
            let npc = &self.actors[me];
            if npc.mind.leader.is_some() && npc.mind.goal == npc.mind.leader {
                self.clear_goal(me);
            }
            if self.level.stub_class_ai {
                self.host.stub(self.actors[me].number, "NPC_BSST_Attack");
            } else {
                self.bs_st_attack(me, command);
            }
            return;
        }
        // A leader followed (`NPC_BSFollowLeader`, `NPC_AI_Default.c:891-961`) where it is
        // the goal, or where there is none.
        let leader = self.actors[me].mind.leader;
        if self.update_goal(me, command).is_some() {
            if leader.is_some()
                && self.actors[me].mind.enemy.is_none()
                && self.actors[me].mind.goal == leader
            {
                self.bs_follow_leader(me, command);
            } else {
                self.default_move(me, command);
            }
        } else if leader.is_some() && self.actors[me].mind.enemy.is_none() {
            self.bs_follow_leader(me, command);
        }
        self.update_angles(me, true, true, command);
    }

    /// `NPC_BSDefault`'s way to its goal (`NPC_AI_Default.c:888-957`): facing its way,
    /// walking unless scripted to run (or going at its enemy), and — forced to march — only
    /// while someone aims at it.
    fn default_move(&mut self, me: usize, command: &mut sjk_protocol::UserCommand) {
        let npc = &mut self.actors[me];
        let flags = npc.script_flags;
        npc.mind.combat_move = false;
        let at_enemy = npc.mind.goal.is_some() && npc.mind.goal == npc.mind.enemy;
        if flags & SCF_FACE_MOVE_DIR == 0 && at_enemy {
            let goal = self.goal_origin(me).unwrap_or([0.0; 3]);
            let npc = &mut self.actors[me];
            let angles = crate::player_angle_math::vector_angles(crate::npc_senses::subtract(
                goal,
                npc.current_origin,
            ));
            npc.desired_yaw = angles[1];
            npc.mind.desired_pitch = angles[0];
        }
        if flags & SCF_RUNNING != 0 || (flags & SCF_WALKING == 0 && at_enemy) {
            command.buttons &= !BUTTON_WALKING;
        } else {
            command.buttons |= BUTTON_WALKING;
        }
        if flags & SCF_FORCED_MARCH != 0 && !self.someone_looking_at(me) {
            return;
        }
        self.move_to_goal(me, true, command);
    }

    /// `NPC_SomeoneLookingAtMe` (`NPC_utils.c:1058-1084`): a playing, armed player with the
    /// NPC in its potentially visible set and within 30 degrees of its view.
    pub(crate) fn someone_looking_at(&self, me: usize) -> bool {
        let body = self.npc(me);
        self.host.players().iter().any(|player| {
            player.session_team != 3
                && !player.spectating
                && player.weapon != 0
                && self.host.in_pvs(body.origin, player.origin)
                && crate::npc_senses::in_fov(&body, player, 30, 30)
        })
    }

    /// `NPC_BSDefault`'s alerts (`NPC_AI_Default.c:752-775`): a big enough alert made by
    /// an enemy of the NPC's makes it its enemy.
    fn notice_alerts(&mut self, me: usize) {
        let body = self.npc(me);
        let sight = self.sight(me);
        let hear = self.actors[me].definition.stats.earshot;
        let alive = self.body(0).is_some_and(|player| player.health > 0);
        let Self { alerts, host, .. } = self;
        let mut senses = crate::npc_world::HostSenses(&mut **host);
        let Some(at) = alerts.check(
            &mut senses,
            &body,
            sight,
            hear,
            None,
            true,
            crate::npc_senses::AEL_DISCOVERED,
            alive,
        ) else {
            return;
        };
        let alert = alerts.events()[at];
        if alert.id == self.actors[me].mind.last_alert_id {
            return;
        }
        if alert.level >= crate::npc_senses::AEL_DISCOVERED
            && self.actors[me].script_flags & SCF_LOOK_FOR_ENEMIES != 0
        {
            let owner = alert.owner.and_then(|owner| self.body(owner));
            if let Some(owner) = owner
                && owner.health >= 0
                && owner.player_team == self.actors[me].enemy_team
            {
                self.set_enemy(me, owner.number);
            }
        }
    }
}
