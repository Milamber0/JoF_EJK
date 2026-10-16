//! What an NPC's thinking keeps between thinks: the reference's `gNPC_t` fields the think
//! loop, the default behaviour and the senses read and write, the `gclient_t` and
//! `gentity_t` fields they share with a player (`renderInfo`, `enemy`, `pers.cmd`), and the
//! NPC's timers (`g_timer.c`).
//!
//! Timers are the reference's named deadlines (`TIMER_Set`, `TIMER_Done`): a list per NPC
//! with no fixed capacity. The names are the reference's own string constants.

use crate::client_idle::Idle;
use crate::npc_spawn::ENTITYNUM_NONE;
use sjk_protocol::UserCommand;

/// An NPC's named timers: each the level time it runs out at.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NpcTimers(Vec<(&'static str, i32)>);

impl NpcTimers {
    /// `TIMER_Set(ent, name, duration)`: the timer runs out `duration` from now.
    pub fn set(&mut self, name: &'static str, level_time: i32, duration: i32) {
        let time = level_time + duration;
        match self
            .0
            .iter_mut()
            .find(|(known, _)| known.eq_ignore_ascii_case(name))
        {
            Some(timer) => timer.1 = time,
            None => self.0.push((name, time)),
        }
    }

    /// `TIMER_Done`: a timer never set is done; one set is done once its time is past.
    pub fn done(&self, name: &str, level_time: i32) -> bool {
        self.get(name).is_none_or(|time| time < level_time)
    }

    /// `TIMER_Done2` (`g_timer.c:252-282`): unlike [`Self::done`], a timer never set is not
    /// done; one run out is removed when `remove` says so.
    pub fn done2(&mut self, name: &str, level_time: i32, remove: bool) -> bool {
        let Some(time) = self.get(name) else {
            return false;
        };
        let done = time < level_time;
        if done && remove {
            self.remove(name);
        }
        done
    }

    /// `TIMER_Get`: when the timer runs out, if it was ever set.
    pub fn get(&self, name: &str) -> Option<i32> {
        self.0
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(name))
            .map(|(_, time)| *time)
    }

    /// `TIMER_Start` (`g_timer.c:308-316`): set only if the timer is done; whether it was.
    pub fn start(&mut self, name: &'static str, level_time: i32, duration: i32) -> bool {
        let done = self.done(name, level_time);
        if done {
            self.set(name, level_time, duration);
        }
        done
    }

    /// Whether no timer was ever set.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `TIMER_Exists`.
    pub fn exists(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// `TIMER_Remove`.
    pub fn remove(&mut self, name: &str) {
        self.0
            .retain(|(known, _)| !known.eq_ignore_ascii_case(name));
    }

    /// Every timer, in the order they were first set.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, i32)> + '_ {
        self.0.iter().copied()
    }
}

/// `ST_ClearTimers` (`NPC_AI_Stormtrooper.c:108-124`): a stormtrooper's timers, all run out
/// now.
pub const STORMTROOPER_TIMERS: [&str; 14] = [
    "chatter",
    "duck",
    "stand",
    "shuffleTime",
    "sleepTime",
    "enemyLastVisible",
    "roamTime",
    "hideTime",
    "attackDelay",
    "stick",
    "scoutTime",
    "flee",
    "interrogating",
    "verifyCP",
];
/// `Jedi_ClearTimers` (`NPC_AI_Jedi.c:115-140`).
pub const JEDI_TIMERS: [&str; 23] = [
    "roamTime",
    "chatter",
    "strafeLeft",
    "strafeRight",
    "noStrafe",
    "walking",
    "taunting",
    "parryTime",
    "parryReCalcTime",
    "forceJumpChasing",
    "jumpChaseDebounce",
    "moveforward",
    "moveback",
    "movenone",
    "moveright",
    "moveleft",
    "movecenter",
    "saberLevelDebounce",
    "noRetreat",
    "holdLightning",
    "gripping",
    "draining",
    "noturn",
];

/// What the NPC's thinking keeps.
#[derive(Clone, Debug, PartialEq)]
pub struct NpcMind {
    /// `NPC->nextBStateThink`: when the behaviour state next runs.
    pub next_bstate_think: i32,
    /// `NPC->tempBehavior`: a behaviour state that overrides the others while set.
    pub temp_behavior: i32,
    /// `NPC->desiredPitch`, `lockedDesiredYaw`, `lockedDesiredPitch`, `aimTime`.
    pub desired_pitch: f32,
    pub locked_desired_yaw: f32,
    pub locked_desired_pitch: f32,
    pub aim_time: i32,
    /// `NPC->combatMove`.
    pub combat_move: bool,
    /// `NPC->desiredSpeed`, `currentSpeed`, `distToGoal`.
    pub desired_speed: i32,
    pub current_speed: i32,
    pub dist_to_goal: f32,
    /// `NPC->last_ucmd`: the command the frames between behaviour thinks repeat.
    pub last_command: UserCommand,
    /// `client->pers.cmd`, `client->lastCmdTime`: the last command thought with, and when.
    pub command: UserCommand,
    pub last_command_time: i32,
    /// `ps.useDelay`: no use key before this time (`g_active.c:2809-2813`, `3368-3372`).
    pub use_delay: i32,
    /// `ent->enemy`, `ent->lastEnemy`: entity numbers.
    pub enemy: Option<u16>,
    pub last_enemy: Option<u16>,
    /// `client->leader`, `NPC->goalEntity`, `NPC->defendEnt`: nobody leads, no goal, nobody
    /// defended, until the steps that set them.
    pub leader: Option<u16>,
    pub goal: Option<u16>,
    pub defend: Option<u16>,
    /// `renderInfo.lookTarget`, `lookTargetClearTime`.
    pub look_target: u16,
    pub look_target_clear_time: i32,
    /// `renderInfo.eyePoint`: where `NPC_Begin` left the NPC's origin, then — for an NPC
    /// without a saber — its view height over where it stood as its frame began
    /// (`UpdateClientRenderinfo`, see [`crate::npc_roster`]).
    pub eye_point: [f32; 3],
    /// `renderInfo.eyeAngles`: its view as its frame began (zero until then, and for a
    /// saber carrier, whose update is the Jedi step's).
    pub eye_angles: [f32; 3],
    /// `NPC->blockedSpeechDebounceTime`: no voice before this time.
    pub blocked_speech_until: i32,
    /// `NPC->currentAim`.
    pub current_aim: i32,
    /// `NPC->lastAlertID`.
    pub last_alert_id: i32,
    /// `NPC->attackHold`, `attackHoldTime`, `ent->attackDebounceTime`.
    pub attack_hold: i32,
    pub attack_hold_time: i32,
    pub attack_debounce_time: i32,
    /// `NPC->confusionTime`, `charmedTime`, `surrenderTime`.
    pub confusion_time: i32,
    pub charmed_time: i32,
    pub surrender_time: i32,
    /// `NPC->greetingDebounceTime`, `ffireCount`, `ffireFadeDebounce`.
    pub greeting_debounce_time: i32,
    pub ffire_count: i32,
    pub ffire_fade_debounce: i32,
    /// `NPC->standTime`: until when a Jedi holds its ground rather than strafing.
    pub stand_time: i32,
    /// `NPC->touchedByPlayer`: the last living client that touched it.
    pub touched_by: Option<u16>,
    /// `ent->cantHitEnemyCounter`.
    pub cant_hit_enemy_counter: i32,
    /// `ps.moveDir`, which is not on the wire: the direction the NPC's own navigation moves
    /// it in. `NPC_Think` clears it every frame.
    pub move_dir: [f32; 3],
    /// `r.currentAngles`: its view after each move.
    pub current_angles: [f32; 3],
    /// `ps.entityEventSequence`, `ent->eventTime`.
    pub shown_events: i32,
    pub event_time: i32,
    /// `client->timeResidual` and `G_CheckClientIdle`'s memory.
    pub time_residual: i32,
    pub idle: Idle,
    /// The NPC's timers.
    pub timers: NpcTimers,
    /// What its fighting keeps ([`crate::npc_combat`], [`crate::npc_pain`],
    /// [`crate::npc_death`]).
    pub fight: NpcFight,
    /// What its squad tactics keep ([`crate::npc_st`]).
    pub tactics: NpcTactics,
    /// `fd.forceJumpSound`: its last move began a Force jump, which its next
    /// `WP_ForcePowersUpdate` sounds.
    pub force_jump_sound: bool,
    /// `client->noclip`: an ambusher hanging from the ceiling (`Jedi_WaitingAmbush`).
    pub noclip: bool,
    /// `ent->useDebounceTime`: when a cultist destroyer blows up (`Jedi_InSpecialMove`).
    pub use_debounce_time: i32,
    /// `NPC->walkDebounceTime`: Boba Fett's gloat over a kill (`Jedi_Attack`).
    pub walk_debounce_time: i32,
    /// `client->jetPackTime`: until when Boba Fett's jet pack burns after a chase jump.
    pub jet_pack_time: i32,
    /// `ps.forceHandExtendTime` and `quickerGetup`: the knockdown's memory, which its
    /// `WP_ForcePowersUpdate` keeps as a player's does.
    pub knockdown: crate::knockdown::Knockdown,
    /// `renderInfo.muzzlePoint`, `muzzlePointOld`: where it stood at its last two render
    /// updates (`UpdateClientRenderinfo`), which a Jedi's evasions read.
    pub muzzle_point: [f32; 3],
    pub muzzle_point_old: [f32; 3],
    /// `ps.saberEntityDist`, `ps.saberEntityState`: how far its thrown saber is out, and
    /// whether it is still leaving (`SES_LEAVING`).
    pub saber_entity_dist: i32,
    pub saber_entity_state: i32,
    /// `sess.saberLevel`: the style its last `WP_ForcePowersUpdate` saw (`w_force.c:5027-5038`),
    /// which `WP_InitForcePowers` gives it as it begins.
    pub session_saber_level: i32,
    /// `client->pushEffectTime`: until when a push's body effect shows (`EF_BODYPUSH`).
    pub push_effect_time: i32,
    /// `ps.saberLockHits`: what a push in a saber lock added to it.
    pub saber_lock_hits: i32,
    /// The reference function its last behaviour think reached that is not ported yet
    /// ([`crate::npc_behavior::Behavior::Stub`]: a later step's class AI or state), in
    /// place of acting; `None` when the think ran ported behaviour. Diagnostics only: the
    /// server names it once, and tests read it.
    pub unported: Option<&'static str>,
    /// The unported function the host last told of for this NPC, so that it tells of each
    /// once.
    pub unported_told: Option<&'static str>,
    /// What a monster's or droid's AI keeps on its entity: its victim, its bolts
    /// ([`crate::npc_creature`]).
    pub creature: crate::npc_creature::Creature,
    /// `renderInfo.lookingDebounceTime`, `renderInfo.lastHeadAngles`: a droid's head eases
    /// toward what it looks at until then, from where it last turned
    /// ([`crate::npc_droid_head`]).
    pub looking_debounce_time: i32,
    pub last_head_angles: [f32; 3],
    /// What the default set's states keep ([`crate::npc_states`]).
    pub states: crate::npc_states::StatesMind,
    /// What a sniper keeps: its enemy's lagged places ([`crate::npc_sniper`]).
    pub sniper: crate::npc_sniper::SniperMind,
}

/// What an NPC's fighting keeps: `gNPC_t`'s shot timing and burst, its local state and
/// time of death; `gentity_t`'s `painDebounceTime` and `pos1`; `gclient_t`'s
/// `respawnTime` and `lastKillTime`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NpcFight {
    /// `painDebounceTime`: no pain animation before it, and a stormtrooper holds its fire.
    pub pain_debounce_time: i32,
    /// `NPC->shotTime`: no shot before it.
    pub shot_time: i32,
    /// `burstCount`, `burstMin`, `burstMean`, `burstMax`, `burstSpacing`: the shots left in
    /// a burst, its size, and the pause after it (the time between shots of a weapon that
    /// does not burst).
    pub burst_count: i32,
    pub burst_min: i32,
    pub burst_mean: i32,
    pub burst_max: i32,
    pub burst_spacing: i32,
    /// `currentAmmo`: the rounds its weapon had when it last fired or was changed.
    pub current_ammo: i32,
    /// `NPC->localState` (`LSTATE_*`): a stormtrooper under fire.
    pub local_state: i32,
    /// `NPC->timeOfDeath`: when it died, then when its body is next looked at.
    pub time_of_death: i32,
    /// `client->respawnTime`: its death animation's end, which its corpse turns
    /// non-solid half a second before.
    pub respawn_time: i32,
    /// `client->lastKillTime`: when it last killed, for the excellent award.
    pub last_kill_time: i32,
    /// `pos1`: where the killing blow landed.
    pub death_point: [f32; 3],
    /// `lasthurt_client`, `lasthurt_mod`: who last hurt it, and how.
    pub last_hurt: Option<(u16, u32)>,
    /// `ps.otherKiller` and its times: who last pushed it, credited with a death that
    /// follows (`player_die`'s crush, fall and unknown deaths).
    pub other_killer: crate::damage::OtherKiller,
    /// `ent->pain`: the pain it was given as it began.
    pub pain: crate::npc_pain::PainFunc,
}

impl Default for NpcMind {
    fn default() -> Self {
        Self {
            next_bstate_think: 0,
            temp_behavior: 0,
            desired_pitch: 0.0,
            locked_desired_yaw: 0.0,
            locked_desired_pitch: 0.0,
            aim_time: 0,
            combat_move: false,
            desired_speed: 0,
            current_speed: 0,
            dist_to_goal: 0.0,
            last_command: UserCommand::default(),
            command: UserCommand::default(),
            last_command_time: 0,
            use_delay: 0,
            enemy: None,
            last_enemy: None,
            leader: None,
            goal: None,
            defend: None,
            look_target: 0,
            look_target_clear_time: 0,
            eye_point: [0.0; 3],
            eye_angles: [0.0; 3],
            blocked_speech_until: 0,
            current_aim: 0,
            last_alert_id: 0,
            attack_hold: 0,
            attack_hold_time: 0,
            attack_debounce_time: 0,
            confusion_time: 0,
            charmed_time: 0,
            surrender_time: 0,
            greeting_debounce_time: 0,
            ffire_count: 0,
            ffire_fade_debounce: 0,
            stand_time: 0,
            touched_by: None,
            cant_hit_enemy_counter: 0,
            move_dir: [0.0; 3],
            current_angles: [0.0; 3],
            shown_events: 0,
            event_time: 0,
            time_residual: 0,
            idle: Idle::default(),
            timers: NpcTimers::default(),
            fight: NpcFight::default(),
            tactics: NpcTactics::default(),
            force_jump_sound: false,
            noclip: false,
            use_debounce_time: 0,
            walk_debounce_time: 0,
            jet_pack_time: 0,
            knockdown: crate::knockdown::Knockdown::default(),
            muzzle_point: [0.0; 3],
            muzzle_point_old: [0.0; 3],
            saber_entity_dist: 0,
            saber_entity_state: 0,
            session_saber_level: 0,
            push_effect_time: 0,
            saber_lock_hits: 0,
            unported: None,
            unported_told: None,
            creature: crate::npc_creature::Creature::default(),
            looking_debounce_time: 0,
            last_head_angles: [0.0; 3],
            states: crate::npc_states::StatesMind::default(),
            sniper: crate::npc_sniper::SniperMind::default(),
        }
    }
}

/// `WAYPOINT_NONE`.
pub const WAYPOINT_NONE: i32 = -1;

/// `NPC->tempGoal`: the goal entity an NPC goes to a place with (`NPC_SetMoveGoal`). The
/// entity itself is the host's ([`crate::npc_spawn::NpcActor::goal`]); what the game
/// keeps on it is here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempGoal {
    /// `r.currentOrigin`, `r.mins`, `r.maxs` (the NPC's mins, twice: `g_nav.c:137-138`).
    pub origin: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// `flags & FL_NAVGOAL`.
    pub nav_goal: bool,
    /// `combatPoint`, `enemy` (the entity it was set for).
    pub combat_point: i32,
    pub target: Option<u16>,
    /// `waypoint`, `lastWaypoint`, `noWaypointTime`.
    pub waypoint: i32,
    pub last_waypoint: i32,
    pub no_waypoint_time: i32,
}

impl Default for TempGoal {
    fn default() -> Self {
        Self {
            origin: [0.0; 3],
            mins: [0.0; 3],
            maxs: [0.0; 3],
            nav_goal: false,
            combat_point: 0,
            target: None,
            waypoint: 0,
            last_waypoint: 0,
            no_waypoint_time: 0,
        }
    }
}

/// What an NPC's tactics keep: `gNPC_t`'s combat point, squad state and group, the goal's
/// radius, what it last saw of its enemy, what it investigates, what it will say when it
/// moves, whom it is blocked by and how it steps round; `gentity_t`'s `waypoint` and
/// `noWaypointTime`; `gclient_t`'s `pushVec`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NpcTactics {
    /// `combatPoint` (-1 for none), `lastFailedCombatPoint` (0 until one fails).
    pub combat_point: i32,
    pub last_failed_combat_point: i32,
    /// `squadState` (`SQUAD_*`), and the index of its group in the level's (`group`).
    pub squad_state: i32,
    pub group: Option<usize>,
    /// `goalRadius`, `goalTime`, `lastPathAngles`.
    pub goal_radius: i32,
    pub goal_time: i32,
    pub last_path_angles: [f32; 3],
    /// The goal entity it keeps.
    pub temp_goal: TempGoal,
    /// `enemyLastSeenTime`, `enemyLastSeenLocation`.
    pub enemy_last_seen_time: i32,
    pub enemy_last_seen_location: [f32; 3],
    /// `investigateDebounceTime`, `investigateCount`, `investigateGoal`,
    /// `investigateSoundDebounceTime`, `pauseTime`.
    pub investigate_debounce_time: i32,
    pub investigate_count: i32,
    pub investigate_goal: [f32; 3],
    pub investigate_sound_debounce_time: i32,
    pub pause_time: i32,
    /// `movementSpeech`, `movementSpeechChance`.
    pub movement_speech: i32,
    pub movement_speech_chance: f32,
    /// `blockingEntNum`, `sideStepHoldTime`, `lastSideStepSide`, `shoveCount`,
    /// `consecutiveBlockedMoves`.
    pub blocking_ent_num: i32,
    pub side_step_hold_time: i32,
    pub last_side_step_side: i32,
    pub shove_count: i32,
    pub consecutive_blocked_moves: i32,
    /// The entity's `waypoint`, `lastWaypoint` and `noWaypointTime`.
    pub waypoint: i32,
    pub last_waypoint: i32,
    pub no_waypoint_time: i32,
    /// The entity's `failedWaypoints` (a node plus one) and `failedWaypointCheckTime`.
    pub failed_waypoints: [i32; crate::npc_navigator::MAX_FAILED_NODES],
    pub failed_waypoint_check_time: i32,
    /// `blockedDest`: where the NPC was going when it was blocked.
    pub blocked_dest: [f32; 3],
    /// `homeWp`: the waypoint a search starts from.
    pub home_waypoint: i32,
    /// `client->pushVec`, `pushVecTime`: another NPC shoving it aside.
    pub push_vec: [f32; 3],
    pub push_vec_time: i32,
    /// `enemyCheckDebounceTime` (`NPC_BSPatrol`).
    pub enemy_check_debounce_time: i32,
}

impl Default for NpcTactics {
    fn default() -> Self {
        Self {
            combat_point: -1,
            last_failed_combat_point: 0,
            squad_state: 0,
            group: None,
            goal_radius: 0,
            goal_time: 0,
            last_path_angles: [0.0; 3],
            temp_goal: TempGoal::default(),
            enemy_last_seen_time: 0,
            enemy_last_seen_location: [0.0; 3],
            investigate_debounce_time: 0,
            investigate_count: 0,
            investigate_goal: [0.0; 3],
            investigate_sound_debounce_time: 0,
            pause_time: 0,
            movement_speech: 0,
            movement_speech_chance: 0.0,
            blocking_ent_num: 0,
            side_step_hold_time: 0,
            last_side_step_side: 0,
            shove_count: 0,
            consecutive_blocked_moves: 0,
            waypoint: 0,
            last_waypoint: 0,
            no_waypoint_time: 0,
            failed_waypoints: [0; crate::npc_navigator::MAX_FAILED_NODES],
            failed_waypoint_check_time: 0,
            blocked_dest: [0.0; 3],
            home_waypoint: 0,
            push_vec: [0.0; 3],
            push_vec_time: 0,
            enemy_check_debounce_time: 0,
        }
    }
}

impl NpcTactics {
    /// The entity's navigation, as the navigator reads it.
    pub fn nav_state(&self) -> crate::npc_navigator::NavState {
        crate::npc_navigator::NavState {
            waypoint: self.waypoint,
            last_waypoint: self.last_waypoint,
            no_waypoint_time: self.no_waypoint_time,
            failed: self.failed_waypoints,
            failed_check_time: self.failed_waypoint_check_time,
        }
    }
}

impl NpcMind {
    /// `NPC_ClearLookTarget` (`NPC_utils.c:1626-1640`), for an NPC not held by a monster.
    pub fn clear_look_target(&mut self) {
        self.look_target = ENTITYNUM_NONE;
        self.look_target_clear_time = 0;
    }

    /// `NPC_SetLookTarget` (`NPC_utils.c:1647-1661`).
    pub fn set_look_target(&mut self, number: u16, clear_time: i32) {
        self.look_target = number;
        self.look_target_clear_time = clear_time;
    }
}
