//! One connected player as the server keeps it: what the game remembers of it between
//! spawns, its wire state, its movement, the entity the others are shown and the clusters
//! that entity touches — and the two things that happen to it over and over: a command
//! (`ClientThink_real`) and the end of a server frame (`ClientEndFrame`).
use crate::collision::{Void, WithPlayers, WorldCollision};
use crate::map::LoadedMap;
use crate::visibility::ClusterLink;
use sjk_game_jka::pmove::{MoveContext, MovementCollision};
use sjk_game_jka::pmove_saber_lock::{LockContext, LockOutcome};
use sjk_game_jka::{
    client_begin::PlayerSession, damage::Wounds, entity_clip::BoxObstacle, entity_id::EntityId,
    entity_pool::EntityPool, player_death::Mortality, player_entity::PlayerEntity,
    pmove::Predictor, saber_frame::SaberFrame, userinfo::AcceptedUserinfo,
};
use sjk_protocol::{PlayerState, UserCommand};

/// `TEAM_SPECTATOR` (`bg_public.h`).
pub(crate) const TEAM_SPECTATOR: u8 = 3;
/// Wire field `eFlags`.
pub(crate) const EFLAGS: usize = 17;
/// `EF_CONNECTION`: draw the connection-trouble sprite over this player.
pub(crate) const EF_CONNECTION: u32 = 1 << 14;
/// Wire field `eventSequence`.
const EVENT_SEQUENCE: usize = 19;

/// What a think's move leaves for the rest of it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Moved {
    events_before: u32,
    time_before: i32,
    /// `r.currentOrigin` before the move: where a key's sounds are.
    pub(crate) origin_before: [f32; 3],
}

/// A connected player of any protocol.
pub struct Peer {
    /// Native mouse-control session; the human form is suspended while it is present.
    /// The name the game kept (`ClientCleanName`), not the one the client sent.
    pub(crate) name: Vec<u8>,
    /// The host the engine saw, as the userinfo's `ip` key carries it (`Bot` for a bot).
    pub(crate) address: Vec<u8>,
    /// A bot (`SVF_BOT`): no transport, its commands the game's own.
    pub(crate) bot: bool,
    /// Its holdables' server-side state: the jetpack and cloak packs, the seeker drone's
    /// clocks (`bridge_holdables`).
    pub(crate) gear: sjk_game_jka::holdables::Gear,
    /// A bot's personality (`BotUtilizePersonality`), which its thinking reads.
    pub(crate) personality: Option<Box<sjk_game_jka::bot_personality::BotPersonality>>,
    /// What every legacy client is told about this player (`CS_PLAYERS`).
    pub(crate) client_info: Vec<u8>,
    /// The userinfo as last accepted, which a begin reads the Force configuration from.
    pub(crate) userinfo: Vec<u8>,
    /// What the game kept of it.
    pub(crate) accepted: AcceptedUserinfo,
    /// Team, Force and saber memory between spawns.
    pub(crate) session: PlayerSession,
    /// `client->siegeClass`: the index of its siege class in the game's registry, as
    /// `ClientUserinfoChanged` last found it; `None` for none (`-1`).
    pub(crate) siege_class_index: Option<usize>,
    /// `switchClassTime`: when it may pick another siege class.
    pub(crate) switch_class_time: i32,
    /// `tempSpectate`: a siege player dead and waiting for its respawn wave until this
    /// level time; zero when it is not waiting.
    pub(crate) temp_spectate: i32,
    /// The siege item it carries and the trigger it hacks (`bridge_siege_items`).
    pub(crate) siege_hands: sjk_game_jka::siege_triggers::SiegeHands,
    /// `siegeEDataSend`: when a stat viewer is next sent its team's health and ammo.
    pub(crate) siege_data_time: i32,
    /// What the vehicle code keeps of the player's entity (`r.ownerNum`, `r.contents`,
    /// `solidHack`, `ps.useDelay`, `FL_VEH_BOARDING`).
    pub(crate) riding: sjk_game_jka::vehicle_rider::RiderBody,
    /// `client->readyToExit`: this player has asked to leave the intermission, and — the
    /// reference's own words — "once a player says ready, it should stick".
    pub(crate) ready_to_exit: bool,
    /// `client->oldbuttons` as `ClientIntermissionThink` latches them, so that holding
    /// the attack button down is not a second request.
    pub(crate) intermission_buttons: i32,
    /// `client->buttons` as a spectator's last think left them: the follow buttons act
    /// on a press, not while held.
    pub(crate) spectator_buttons: u16,
    /// The player's state as legacy clients are sent it; movement writes its part after
    /// every command, the game owns the rest.
    pub(crate) state: PlayerState,
    /// Server time at which spawn protection ends (`invulnerableTimer`).
    pub(crate) invulnerable_until: i32,
    /// What the other clients are shown of the player, and whether they are: a
    /// spectator's entity is not linked into the world, so no snapshot lists it.
    pub(crate) entity: PlayerEntity,
    /// The clusters the player's box touches, for the PVS test of every snapshot.
    pub(crate) link: ClusterLink,
    /// `pers.connected == CON_CONNECTED`: the player has begun.
    pub(crate) begun: bool,
    /// `pers.enterTime`: when it did.
    pub(crate) enter_time: i32,
    /// `respawnTime` and the like.
    pub(crate) mortality: Mortality,
    /// `gentity_t::health`: the game's, copied into the stat at every frame's end.
    pub(crate) health: i32,
    /// A corpse's contents and the top of its box (`r.contents`, `r.maxs[2]`), while dead.
    pub(crate) corpse: Option<(u32, f32)>,
    /// The saber as the frames keep it: its throw delay and the style queued.
    pub(crate) saber: SaberFrame,
    /// Its saber entity out of its hand: what the game keeps of the flight, and of the
    /// throw on the thrower (`saberEntityState`, `saberDidThrowTime`).
    pub(crate) flight: sjk_game_jka::saber_throw::SaberEntity,
    /// `saberStoredIndex`: its saber entity, which the state stops naming while the saber
    /// is knocked out of its hand.
    pub(crate) saber_entity: Option<EntityId>,
    /// Its two sabers (`client->saber`), and the names others are told they are.
    pub(crate) sabers: sjk_game_jka::player_sabers::PlayerSabers,
    pub(crate) throw_memory: sjk_game_jka::saber_throw::ThrowMemory,
    /// The skeleton the server poses for it (`ent->ghoul2`), once a map with models has
    /// given it one.
    pub(crate) skeleton: Option<crate::bridge::PlayerSkeleton>,
    /// Where its blade was last read (`lastSaberBase_Always`, `lastSaberDir_Always`),
    /// and when; `None` until the saber has been held out.
    pub(crate) blade: Option<(sjk_game_jka::server_skeleton::Blade, i32)>,
    /// Every blade of both hilts as last read, and when (`saber[n].blade[m]`'s muzzle).
    pub(crate) blades: BladeReadings,
    /// The readings before [`Self::blades`] (`muzzlePointOld`, `muzzleDirOld`, copied before
    /// each new reading: `w_saber.c:8836-8837`), which a Jedi NPC's blocks extrapolate from.
    pub(crate) blades_old: BladeReadings,
    /// `renderInfo.muzzlePoint`, `renderInfo.muzzlePointOld` (`UpdateClientRenderinfo`,
    /// `w_saber.c:7468-7470`): its origin at its last saber update, and the one before.
    pub(crate) muzzle: ([f32; 3], [f32; 3]),
    /// What its blade's damage remembers between frames.
    pub(crate) saber_cut: crate::bridge::SaberCut,
    /// Server time of the last command (`lastCmdTime`): a player silent for a second
    /// is shown with the connection-trouble flag.
    pub(crate) last_command_time: i32,
    /// The last command received (`pers.cmd`), which a spawn without one thinks with.
    pub(crate) last_command: UserCommand,
    /// What the frame's damage did to it, for the feedback at the frame's end.
    pub(crate) wounds: Wounds,
    /// Its air under water and its next drowning blow (`airOutTime`, `ent->damage`).
    pub(crate) breath: sjk_game_jka::world_effects::Breath,
    /// What the game keeps of a saber lock beside the wire (`ps.saberLockHits` and its
    /// times) and the last two commands' buttons.
    pub(crate) lock: sjk_game_jka::saber_lock::LockMemory,
    /// `ps.painTime`, `ps.painDirection`: the pain sound's pacing.
    pub(crate) pain: (i32, bool),
    /// `timeResidual`: the living player's seconds, for the once-a-second actions.
    pub(crate) time_residual: i32,
    /// `G_CheckClientIdle`'s memory.
    pub(crate) idle: sjk_game_jka::client_idle::Idle,
    /// The knockdown's memory (`forceHandExtendTime`, `quickerGetup`, `otherKiller`).
    pub(crate) knockdown: sjk_game_jka::knockdown::Knockdown,
    /// The Force no client is sent (`forcedata_t`'s levels, timers and sound trackers).
    pub(crate) force: sjk_game_jka::force_powers::ForcePowers,
    /// `pushEffectTime`: until when a Force push's full-body effect shows.
    pub(crate) push_effect_until: i32,
    /// `lastGenCmd`, `lastGenCmdTime`.
    pub(crate) generic: sjk_game_jka::generic_commands::GenericMemory,
    /// `pers.teamState`: its capture-the-flag record.
    pub(crate) team_state: sjk_game_jka::ctf::TeamState,
    /// `saberBlockTime`: no saber block before this time.
    pub(crate) block_time: i32,
    /// `accuracy_hits`, `accuracy_shots`.
    pub(crate) accuracy: (i32, i32),
    /// `r.broadcastClients`: the clients it is sent to wherever they are, a bit each
    /// (`G_UpdateClientBroadcasts`, at its every think).
    pub(crate) broadcast_to: Vec<u64>,
    /// `iAmALoser`: a power duellist dead this round, waiting in line.
    pub(crate) loser: bool,
    /// `pers.netnameTime`: before this level time a new name is refused.
    pub(crate) netname_time: i32,
    /// `ps.ping`, as the endpoint last measured it; the scoreboard and `getstatus`
    /// report it. A player never measured holds 0.
    pub(crate) ping: i32,
    /// `switchDuelTeamTime`: when the `duelteam` command is next heard.
    pub(crate) switch_duel_team_time: i32,
    /// `ent->enemy` as the NPCs' code sets a player's (`NPC_CheckAttacker`): the NPC it is
    /// taken to be fighting.
    pub(crate) npc_enemy: Option<u16>,
    /// `inSpaceIndex` and `inSpaceSuffocation`: the space trigger it is in, and when it
    /// next misses a breath (`sjk_game_jka::vehicle_triggers`).
    pub(crate) in_space: u16,
    pub(crate) suffocation: i32,
    /// `noCorpse`: it died in space or in its fighter, and leaves no body.
    pub(crate) no_corpse: bool,
    /// `client->noclip`: the `noclip` cheat is on ([`sjk_game_jka::noclip`]). A spawn
    /// clears it with the rest of the client.
    pub(crate) noclip: bool,
    /// The player's movement, simulated with the rules the client predicts with.
    /// Spectating is the only state with the evidence a server needs so far.
    pub(crate) movement: Predictor,
}

/// Every blade of a player's two hilts as the pose last read them, and the frame.
pub(crate) type BladeReadings = (
    [[Option<sjk_game_jka::server_skeleton::Blade>; sjk_game_jka::server_skeleton::MAX_BLADES]; 2],
    i32,
);

/// Who a player's move meets besides the map: the others' boxes, the legs each player
/// and NPC stands on by number (for the saber's specials, `PM_BGEntForNum`, and the bounce
/// off an NPC's head), and the game type.
#[derive(Clone, Copy)]
pub(crate) struct Others<'a> {
    pub(crate) boxes: &'a [BoxObstacle],
    pub(crate) legs: &'a [Option<u16>],
    pub(crate) gametype: i32,
    /// The Ghoul2 clock the player's own model is read at (`PM_FootSlopeTrace`'s feet).
    pub(crate) ghoul2_time: i32,
}

/// `FP_SABER_OFFENSE`; defense and throw follow it.
const FP_SABER_OFFENSE: usize = 15;

impl Peer {
    /// A client as `ClientConnect` leaves it (`memset` of `gclient_t`, then its `pers`
    /// and session): nothing of a previous level, not yet begun.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn connected(
        client: usize,
        accepted: AcceptedUserinfo,
        address: Vec<u8>,
        userinfo: Vec<u8>,
        session: PlayerSession,
        state: PlayerState,
        movement: Predictor,
    ) -> Self {
        Self {
            siege_class_index: None,
            temp_spectate: 0,
            siege_hands: Default::default(),
            siege_data_time: 0,
            riding: Default::default(),
            ready_to_exit: false,
            intermission_buttons: 0,
            spectator_buttons: 0,
            switch_class_time: 0,
            name: accepted.name.clone(),
            address,
            bot: false,
            gear: Default::default(),
            personality: None,
            client_info: accepted.client_info.clone(),
            userinfo,
            accepted,
            session,
            state,
            invulnerable_until: 0,
            entity: PlayerEntity::new(client as u16),
            link: ClusterLink::default(),
            begun: false,
            enter_time: 0,
            mortality: Mortality::default(),
            health: 0,
            corpse: None,
            saber: SaberFrame::default(),
            flight: Default::default(),
            saber_entity: None,
            sabers: Default::default(),
            throw_memory: Default::default(),
            skeleton: None,
            blade: None,
            blades: ([[None; sjk_game_jka::server_skeleton::MAX_BLADES]; 2], 0),
            blades_old: ([[None; sjk_game_jka::server_skeleton::MAX_BLADES]; 2], 0),
            muzzle: ([0.0; 3], [0.0; 3]),
            saber_cut: Default::default(),
            block_time: 0,
            last_command: UserCommand::default(),
            wounds: Wounds::default(),
            breath: Default::default(),
            lock: Default::default(),
            pain: (0, false),
            time_residual: 0,
            idle: sjk_game_jka::client_idle::Idle::default(),
            knockdown: sjk_game_jka::knockdown::Knockdown::default(),
            force: sjk_game_jka::force_powers::ForcePowers::with_levels(
                [0; sjk_game_jka::force_powers::NUM_FORCE_POWERS],
            ),
            push_effect_until: 0,
            generic: Default::default(),
            team_state: Default::default(),
            accuracy: (0, 0),
            broadcast_to: Vec::new(),
            loser: false,
            ping: 0,
            netname_time: 0,
            switch_duel_team_time: 0,
            npc_enemy: None,
            in_space: 0,
            suffocation: 0,
            no_corpse: false,
            noclip: false,
            last_command_time: 0,
            movement,
        }
    }

    /// `saber[0].soundOff`, and the second saber's where one is held: what its death
    /// plays as its sabers go out.
    pub(crate) fn saber_off_sounds(&self) -> [u16; 2] {
        let [first, second] = &self.sabers.hands;
        [
            first.sound_off,
            if second.is_held() {
                second.sound_off
            } else {
                0
            },
        ]
    }

    /// Its saber entity's legacy number (`saberEntityNum` as the wire and the traces
    /// know it), 0 for none.
    pub(crate) fn saber_number(&self) -> u16 {
        self.saber_entity.map_or(0, EntityId::legacy_number)
    }

    /// One movement command: simulated, written to the wire state, and copied to the
    /// player's entity as `ClientThink_real` ends. Returns whether the command ran: one
    /// not ahead of the player's time (clients resend recent commands) does nothing, and
    /// nothing that follows a think — its events, its fire — may happen for it.
    pub(crate) fn think(
        &mut self,
        command: UserCommand,
        map: Option<&LoadedMap>,
        others: Others,
        pool: &mut EntityPool,
        server_time: i32,
        rng: &mut sjk_game_jka::player_death::Rng,
    ) -> bool {
        let (moved, _) = self.move_command(command, map, others, server_time, None);
        self.after_move(command, moved, map, pool, server_time, rng)
    }

    /// The first part of [`Self::think`]: the command kept and the move run. What
    /// follows it — the key's generic command, then [`Self::after_move`] — needs it.
    /// A player in a saber lock moves with `lock`, which reaches the opponent's
    /// movement; what the lock's break asks of the game comes back.
    pub(crate) fn move_command(
        &mut self,
        command: UserCommand,
        map: Option<&LoadedMap>,
        others: Others,
        server_time: i32,
        lock: Option<LockContext>,
    ) -> (Moved, LockOutcome) {
        // `ClientThink` marks the time before anything else, the spawn's own think too.
        self.last_command_time = server_time;
        self.last_command = command;
        let moved = Moved {
            events_before: self.state.raw_field(EVENT_SEQUENCE).unwrap_or(0),
            time_before: self.state.command_time(),
            origin_before: self.state.origin(),
        };
        let Others {
            boxes: players,
            legs,
            gametype,
            ghoul2_time,
        } = others;
        let bodies = |number: u16| legs.get(usize::from(number)).copied().flatten();
        // The NPCs are the bodies past the clients' numbers.
        let npcs = |number: u16| number >= 32 && bodies(number).is_some();
        // `BG_MySaber`'s speed scales, as the sabers stand now.
        let (animation, movement) = self.sabers.speed_scales();
        self.movement.set_saber_scales(animation, movement);
        let levels = &self.force.levels;
        let context = MoveContext {
            saber_offense: levels[FP_SABER_OFFENSE],
            saber_defense: levels[FP_SABER_OFFENSE + 1],
            saber_throw: levels[FP_SABER_OFFENSE + 2],
            sabers: self.sabers.movement(),
            gametype,
            saber_throws: true,
            bodies: &bodies,
            npcs: &npcs,
            foot_bolts: &sjk_game_jka::pmove::no_foot_bolts,
        };
        // `PM_FootSlopeTrace` on the player's own model (`pm->ghoul2`), at the Ghoul2 clock:
        // lent to the move and put back after it.
        let skeleton = std::cell::RefCell::new(self.skeleton.take());
        let outcome = {
            let feet = |origin: [f32; 3], yaw: f32| {
                skeleton
                    .borrow_mut()
                    .as_mut()?
                    .foot_points(yaw, origin, ghoul2_time)
            };
            let context = context.with_foot_bolts(&feet);
            match (map, lock) {
                (Some(map), lock) => {
                    let world = WithPlayers {
                        world: WorldCollision {
                            bsp: &map.bsp,
                            scratch: &map.scratch,
                        },
                        players,
                    };
                    self.run_move(command, &world, &context, lock)
                }
                (None, lock) => self.run_move(
                    command,
                    &WithPlayers {
                        world: Void,
                        players,
                    },
                    &context,
                    lock,
                ),
            }
        };
        self.skeleton = skeleton.into_inner();
        self.movement.write_player_state(&mut self.state);
        self.movement.copied_to_entity();
        (moved, outcome)
    }

    /// The move itself: the locked one for a player in a saber lock.
    fn run_move(
        &mut self,
        command: UserCommand,
        world: &impl MovementCollision,
        context: &MoveContext,
        lock: Option<LockContext>,
    ) -> LockOutcome {
        match lock {
            Some(lock) => self
                .movement
                .predict_command_locked(command, world, context, lock),
            None => {
                self.movement.predict_command_in(command, world, context);
                LockOutcome::default()
            }
        }
    }

    /// The rest of [`Self::think`] after the move: the entity converted, the second's
    /// actions and the idle check, the entity linked. Returns whether the command ran.
    pub(crate) fn after_move(
        &mut self,
        command: UserCommand,
        moved: Moved,
        map: Option<&LoadedMap>,
        pool: &mut EntityPool,
        server_time: i32,
        rng: &mut sjk_game_jka::player_death::Rng,
    ) -> bool {
        let Moved {
            events_before,
            time_before,
            ..
        } = moved;
        // A command that is not ahead of the player's time returns before any of this.
        let ran = self.state.command_time() != time_before;
        if ran {
            // An event the entity could not carry leaves in a temp entity for everyone
            // but this player, who predicted it (`SendPendingPredictableEvents`).
            let client = self.entity.state().number();
            if let Some(overflow) =
                self.entity
                    .thought(&self.state, !self.playing(), server_time, events_before)
            {
                let _ = pool.spawn_temporary(overflow.clone(), server_time, Some(client));
            }
            // `ClientTimerActions` for the living: the think's milliseconds, at most 200;
            // then `G_CheckClientIdle`, whose animation the wire state takes now and the
            // entity at the next think.
            if self.playing() && self.health > 0 {
                let msec = (command.server_time - time_before).min(200);
                sjk_game_jka::client_timer::timer_actions(
                    &mut self.state,
                    &mut self.health,
                    &mut self.time_residual,
                    msec,
                );
                let (armor, spectating) = (self.state.stats[5] as i32, !self.playing());
                if sjk_game_jka::client_idle::check_idle(
                    &mut self.idle,
                    &mut self.movement,
                    self.health,
                    armor,
                    spectating,
                    &command,
                    server_time,
                    rng,
                ) {
                    self.movement.write_player_state(&mut self.state);
                }
            }
        }
        // `ClientThink_real` links the entity with the move's box; a spectator's is not
        // linked at all, and a corpse is `CONTENTS_CORPSE`, which no client clips against.
        let bounds = self.movement.box_bounds();
        self.entity
            .linked(bounds, self.playing() && self.corpse.is_none());
        // `client->oldbuttons`, `client->buttons` at the think's end (`g_active.c:3395`).
        self.lock.latch(command.buttons);
        if let Some(map) = map {
            let origin = self.state.origin();
            // `SV_LinkEntity` grows the box by a unit before it asks which leaves it touches.
            let (absmin, absmax) = (
                std::array::from_fn(|axis| origin[axis] + bounds.0[axis] - 1.0),
                std::array::from_fn(|axis| origin[axis] + bounds.1[axis] + 1.0),
            );
            self.link = ClusterLink::new(&map.bsp, absmin, absmax);
        }
        ran
    }

    /// `ClientEvents`' `EV_SABER_ATTACK`: whether the last command swung the saber.
    pub(crate) fn swung(&self) -> bool {
        const EV_SABER_ATTACK: u16 = 29;
        self.movement
            .command_events()
            .any(|event| event.event == EV_SABER_ATTACK)
    }

    /// `ClientEvents`' fire events of the last command: whether each was the alternate
    /// fire. The weapon is fired from the entity as this think converted it.
    pub(crate) fn fire_events(&self) -> impl Iterator<Item = bool> + '_ {
        use sjk_game_jka::weapon_fire::{EV_ALT_FIRE, EV_FIRE_WEAPON};
        self.movement
            .command_events()
            .filter(|event| matches!(event.event, EV_FIRE_WEAPON | EV_ALT_FIRE))
            .map(|event| event.event == EV_ALT_FIRE)
    }

    /// `ClientEvents`' item uses of the last command: each `EV_USE_ITEM1..11`'s holdable.
    pub(crate) fn item_uses(&self) -> impl Iterator<Item = i32> + '_ {
        use sjk_game_jka::holdables::EV_USE_ITEM0;
        self.movement
            .command_events()
            .filter(|event| (EV_USE_ITEM0 + 1..=EV_USE_ITEM0 + 11).contains(&event.event))
            .map(|event| i32::from(event.event - EV_USE_ITEM0))
    }

    /// `ClientEvents`' landings of the last command: the size of each `EV_FALL`.
    pub(crate) fn fall_events(&self) -> impl Iterator<Item = i32> + '_ {
        const EV_FALL: u16 = 11;
        self.movement
            .command_events()
            .filter(|event| event.event == EV_FALL)
            .map(|event| i32::from(event.parameter))
    }

    /// Whether the player is in the world for others: not a spectator, and not a siege
    /// player waiting for its respawn wave (`tempSpectate`).
    pub(crate) fn playing(&self) -> bool {
        self.session.team != i32::from(TEAM_SPECTATOR) && self.temp_spectate == 0
    }

    /// Whether the human form currently participates in physics and world interaction.
    pub(crate) fn body_active(&self) -> bool {
        self.playing()
    }

    /// `ClientEndFrame`: connection trouble is flagged after a second without commands,
    /// and the entity is converted again.
    pub(crate) fn end_frame(&mut self, pool: &mut EntityPool, server_time: i32) {
        if !self.playing() {
            return;
        }
        let flags = self.state.raw_field(EFLAGS).unwrap_or(0);
        let silent = server_time - self.last_command_time > 1_000;
        self.state.set_raw_field(
            EFLAGS,
            if silent {
                flags | EF_CONNECTION
            } else {
                flags & !EF_CONNECTION
            },
        );
        // `P_DamageFeedback`: what the frame's damage tells the player, the pain sound.
        if sjk_game_jka::damage::damage_feedback(
            &mut self.state,
            &mut self.wounds,
            self.health,
            server_time,
            &mut self.pain.0,
            &mut self.pain.1,
        ) {
            self.entity.event_raised(server_time);
        }
        // `ent->client->ps.stats[STAT_HEALTH] = ent->health`.
        self.state.stats[0] = self.health as u32;
        self.movement.set_health(self.health);
        let client = self.entity.state().number();
        if let Some(overflow) = self.entity.frame_ended(&self.state, false) {
            let _ = pool.spawn_temporary(overflow.clone(), server_time, Some(client));
        }
    }
}
