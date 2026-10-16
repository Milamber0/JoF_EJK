//! An NPC's command run as a player's is: `ClientThink` and the NPC's parts of
//! `ClientThink_real` (`codemp/game/g_active.c:1867-3500`) — the command's time, the saber
//! stance, the movement type, the NPC's own speed (`NPC_Accelerate`, its walk and run,
//! its slowing on turns), gravity, `Pmove` through the same movement code as a player's
//! with the NPC's box and class ([`crate::pmove::npc::NpcBody`]), the `ET_NPC` entity
//! converted from the player state, the once-a-second actions and the idle animations.
//! Also what `G_RunFrame` does for every NPC each frame (`g_main.c:3080-3100`,
//! `3333-3346`), and `NPC_SetAnim`.
//!
//! Its fire events fire its weapon (`ClientEvents`, [`crate::npc_combat`]). Left for later
//! steps, and said so: using things (`TryUse`), touching triggers and whatever it walks into
//! (`G_TouchTriggers`, `ClientImpacts`), who else it is broadcast to (a Force-sight user
//! out of sight, `G_UpdateClientBroadcasts`), a siege class's stances, an ambusher's hang
//! (`noclip`), and a spectator NPC.

use crate::npc_spawn::{NpcActor, NpcHost, es};
use crate::npc_world::NpcWorld;
use crate::pmove::npc::NpcBody;
use crate::pmove::{MoveContext, MovementConfig, Predictor};
use sjk_protocol::{EntityState, UserCommand};

/// `ps.` wire fields the think writes.
const PS_EFLAGS: usize = 17;
const PS_EVENT_SEQUENCE: usize = 19;
const PS_EFLAGS2: usize = 103;
const PS_SABER_LOCK_FRAME: usize = 108;
const PS_HELD_BY_CLIENT: usize = 121;
const PS_EXTERNAL_EVENT: usize = 56;
/// `s.legsAnim`, `s.torsoAnim`, `s.event`.
const ES_LEGS_ANIM: usize = 16;
const ES_TORSO_ANIM: usize = 17;
const ES_EVENT: usize = 28;
/// `STAT_HEALTH`, `STAT_HOLDABLE_ITEMS`, `STAT_ARMOR`; `HI_JETPACK`.
const STAT_HEALTH: usize = 0;
const STAT_HOLDABLE_ITEMS: usize = 2;
const STAT_ARMOR: usize = 5;
const HI_JETPACK: u32 = 7;
/// `EF_JETPACK`, `EF_JETPACK_ACTIVE`, `EF_JETPACK_FLAMING`, `EF_INVULNERABLE`,
/// `EF_DISINTEGRATION`; `EF2_FLYING`.
const EF_JETPACK: u32 = 1 << 29;
const EF_JETPACK_ACTIVE: u32 = 1 << 11;
const EF_JETPACK_FLAMING: u32 = 1 << 30;
const EF_INVULNERABLE: u32 = 1 << 27;
const EF_DISINTEGRATION: u32 = 1 << 26;
const EF2_FLYING: u32 = 1 << 4;
const EF2_SHIP_DEATH: u32 = 1 << 7;
/// `EF_BODYPUSH`.
const EF_BODYPUSH: u32 = 1 << 19;
/// `PM_NORMAL`, `PM_NOCLIP`, `PM_DEAD`.
const PM_NORMAL: u8 = 0;
const PM_NOCLIP: u8 = 3;
const PM_DEAD: u8 = 5;
/// `BUTTON_WALKING`.
const BUTTON_WALKING: u16 = 16;
/// `BUTTON_USE`.
const BUTTON_USE: u16 = crate::use_key::BUTTON_USE;
/// `NPCAI_CUSTOM_GRAVITY`, `NPCAI_NO_SLOWDOWN`.
const NPCAI_CUSTOM_GRAVITY: u32 = 0x20_0000;
const NPCAI_NO_SLOWDOWN: u32 = 0x1000;
/// `SLOWDOWN_DIST`, `MIN_NPC_SPEED` (`g_active.c:2134-2135`).
const SLOWDOWN_DIST: f32 = 128.0;
const MIN_NPC_SPEED: f32 = 16.0;
/// `EVENT_VALID_MSEC`.
const EVENT_VALID_MS: i32 = 300;
/// `ET_NPC`.
const ET_NPC: u32 = 13;
/// `TEAM_SPECTATOR`.
const TEAM_SPECTATOR: i32 = 3;
/// `FP_SABER_OFFENSE`, whose defense and throw follow.
const FP_SABER_OFFENSE: usize = 15;
/// The droids whose run the game names (`NPC_GetRunSpeed`).
const PLAIN_RUNNERS: [i32; 11] = [32, 11, 34, 35, 23, 24, 33, 1, 29, 41, 39];
/// `CLASS_R2D2`, `CLASS_R5D2`, `CLASS_MARK2`, `CLASS_MOUSE`, `CLASS_PROBE`: the droids that
/// hum as they move (`G_CheckMovingLoopingSounds`), and their hum.
const HUMMING: [(i32, &[u8]); 5] = [
    (34, b"sound/chars/r2d2/misc/r2_move_lp.wav"),
    (35, b"sound/chars/r2d2/misc/r2_move_lp2.wav"),
    (24, b"sound/chars/mark2/misc/mark2_move_lp"),
    (29, b"sound/chars/mouse/misc/mouse_lp"),
    (32, b"sound/chars/probe/misc/probedroidloop"),
];

/// The movement an NPC moves with: a player's, authoritative.
pub fn movement_config() -> MovementConfig {
    MovementConfig {
        authoritative: true,
        ..Default::default()
    }
}

/// `G_RunFrame`'s part for every NPC, begun or not (`g_main.c:3080-3100`, `3333-3341`):
/// its event cleared once shown for 300 ms, and its powerups that ran out gone.
pub fn frame_upkeep(npc: &mut NpcActor, level_time: i32) {
    if level_time - npc.mind.event_time > EVENT_VALID_MS
        && npc.state.raw_field(ES_EVENT).unwrap_or(0) != 0
    {
        npc.state.set_raw_field(ES_EVENT, 0);
        npc.player.set_raw_field(PS_EXTERNAL_EVENT, 0);
    }
    for powerup in &mut npc.player.powerups {
        if (*powerup as i32) < level_time {
            *powerup = 0;
        }
    }
}

/// `NPC_GetWalkSpeed`, `NPC_GetRunSpeed` (`g_active.c:1454-1508`): a droid runs at its
/// run speed; anyone else at 1.3 times it ("seems to slow in MP").
fn walk_speed(npc: &NpcActor) -> i32 {
    npc.definition.stats.walk_speed
}

fn run_speed(npc: &NpcActor) -> i32 {
    let run = npc.definition.stats.run_speed;
    if PLAIN_RUNNERS.contains(&npc.definition.client_class) {
        run
    } else {
        (run as f32 * 1.3) as i32
    }
}

/// `NPC_Accelerate(ent, qfalse, qfalse)` (`g_active.c:1392-1448`).
fn accelerate(npc: &mut NpcActor) {
    let (acceleration, walk) = (
        npc.definition.stats.acceleration,
        npc.definition.stats.walk_speed,
    );
    let mind = &mut npc.mind;
    if acceleration == 0 {
        mind.current_speed = mind.desired_speed;
    } else if mind.desired_speed <= walk {
        if mind.desired_speed > mind.current_speed + acceleration {
            mind.current_speed += acceleration;
        } else if mind.desired_speed > mind.current_speed || mind.desired_speed < mind.current_speed
        {
            mind.current_speed = mind.desired_speed;
        }
    } else if mind.desired_speed > mind.current_speed || mind.desired_speed < mind.current_speed {
        mind.current_speed = mind.desired_speed;
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `ClientThink(npc, command)` and `ClientThink_real` for the NPC at `me`.
    pub fn client_think(&mut self, me: usize, command: UserCommand) {
        let level_time = self.level_time;
        let gravity = self.host.gravity();
        let npc = &mut self.actors[me];
        npc.mind.last_command_time = level_time;
        npc.mind.command = command;
        // `ClientThink_real`'s first test (`g_active.c:1880-1888`): an NPC no longer
        // `ET_NPC` (a remover gone invisible, `NPC_BSRemove`) is an unconnected client, and
        // thinks no further.
        if npc.state.raw_field(crate::npc_spawn::es::TYPE) != Some(ET_NPC) {
            return;
        }
        // `G_HeldByMonster` (`g_active.c:2000-2003`): a victim a monster holds hangs where it
        // is held ([`crate::npc_rancor_attack`]).
        if crate::npc_rancor::held(
            npc.player
                .raw_field(crate::npc_creature::PS_EFLAGS2)
                .unwrap_or(0),
        ) {
            self.held_by_monster(me);
        }
        let npc = &mut self.actors[me];
        let mut base = npc.movement.state().saber_anim_level_base;
        crate::saber_stance::upkeep(&mut npc.player, &npc.definition.sabers, &mut base);
        npc.movement.set_saber_anim_level_base(base);
        let command_time = npc.player.command_time();
        let command = &mut npc.mind.command;
        command.server_time = command
            .server_time
            .min(level_time + 200)
            .max(level_time - 1_000);
        if command.server_time - command_time < 1 {
            command.server_time = command_time + 100;
        }
        let msec = (command.server_time - command_time).min(200);
        self.movement_type(me);
        self.speed(me);
        let npc = &mut self.actors[me];
        if npc.ai_flags & NPCAI_CUSTOM_GRAVITY == 0 {
            npc.player
                .set_gravity(crate::vehicle_think::gravity(npc).unwrap_or(gravity as i32));
            // Dead in its ship (a droid unit's, `EF2_SHIP_DEATH`): it floats there
            // (`g_active.c:2392-2396`).
            if npc.vehicle.is_none()
                && npc.player.raw_field(PS_EFLAGS2).unwrap_or(0) & EF2_SHIP_DEATH != 0
            {
                npc.player.set_velocity([0.0; 3]);
                npc.player.set_gravity(1);
            }
        }
        npc.player.set_raw_field(PS_HELD_BY_CLIENT, 0);
        // A lock's part (`g_active.c:2917-3001`): its lock's frame cleared outside one.
        self.npc_lock_think(me);
        let npc = &mut self.actors[me];
        // Whoever pushed it is forgotten once it is back on the ground (`g_active.c:2789-2803`).
        let grounded = npc.player.ground_entity_num() != crate::npc_spawn::ENTITYNUM_NONE;
        npc.mind.fight.other_killer.end_frame(grounded, level_time);
        // "when in a vehicle, debounce the use" (`g_active.c:2809-2813`) — a vehicle, whose
        // `m_iVehicleNum` is its pilot, too.
        if npc.mind.use_delay > level_time && npc.player.vehicle_entity_num() != 0 {
            npc.mind.command.buttons &= !BUTTON_USE;
        }
        push_command(npc, level_time);
        self.loop_sounds(me);
        let events_before = self.actors[me]
            .player
            .raw_field(PS_EVENT_SEQUENCE)
            .unwrap_or(0);
        self.run_move(me);
        let npc = &mut self.actors[me];
        npc.mind.current_angles = npc.player.view_angles();
        if npc.player.raw_field(PS_EVENT_SEQUENCE).unwrap_or(0) != events_before {
            npc.mind.event_time = level_time;
        }
        self.convert(me);
        let npc = &mut self.actors[me];
        // "let vehicles that are getting broken apart do their own crazy sizing stuff"
        // (`g_active.c:3348-3355`).
        if npc
            .vehicle
            .as_deref()
            .is_none_or(|vehicle| vehicle.removed_surfaces == 0)
        {
            (npc.mins, npc.maxs) = npc.movement.box_bounds();
        }
        // Linked where the entity is (`s.pos.trBase`), then placed where the move left it.
        npc.relink();
        // `ClientEvents`: the weapon fired from the entity as converted.
        self.fire_events(me);
        // `TryUse` (`g_active.c:3368-3372`): the key's delay. What an NPC's use reaches is
        // left for a later step; a vehicle's own use — driven with the key down — only
        // spends the delay, which debounces its next command.
        let npc = &mut self.actors[me];
        if npc.mind.command.buttons & BUTTON_USE != 0 && npc.mind.use_delay < level_time {
            npc.mind.use_delay = level_time + 100;
        }
        // `G_TouchTriggers`: the items it stands in ([`crate::npc_weapon_pickup`]) and the
        // knocked sabers ([`crate::npc_saber_throw`]).
        if !self.actors[me].mind.noclip {
            self.touch_items(me);
        }
        self.touch_downed_sabers(me);
        self.touch_ship_triggers(me);
        // The map's other triggers, the host's ([`crate::npc_triggers`]).
        if crate::npc_triggers::touches_triggers(&self.actors[me]) {
            let Self { actors, host, .. } = self;
            host.touch_map_triggers(&actors[me]);
        }
        let npc = &mut self.actors[me];
        npc.current_origin = npc.player.origin();
        // `ClientImpacts`: the NPCs its move touched.
        self.client_impacts(me);
        let npc = &mut self.actors[me];
        // `ClientTimerActions`, then `G_CheckClientIdle` (which leaves the dead alone), whose
        // animation the entity shows from the next think.
        crate::client_timer::timer_actions(
            &mut npc.player,
            &mut npc.health,
            &mut npc.mind.time_residual,
            msec,
        );
        let armor = npc.player.stats[STAT_ARMOR] as i32;
        let spectating = npc.session_team == TEAM_SPECTATOR;
        // An NPC's weapon is never "changing" (`ent->s.eType != ET_NPC`).
        let command = UserCommand {
            weapon: npc.player.weapon(),
            ..npc.mind.command
        };
        if crate::client_idle::check_idle(
            &mut npc.mind.idle,
            &mut npc.movement,
            npc.health,
            armor,
            spectating,
            &command,
            level_time,
            self.host.rng(),
        ) {
            write_back(npc);
        }
        // `client->buttons`, `oldbuttons`, which a lock's press is told by (`g_active.c:3395`).
        npc.saber.lock.latch(command.buttons);
    }

    /// `ClientThink_real`'s flags and movement type (`g_active.c:2076-2131`).
    fn movement_type(&mut self, me: usize) {
        let npc = &mut self.actors[me];
        let mut flags = npc.player.raw_field(PS_EFLAGS).unwrap_or(0);
        // Nothing gives an NPC spawn protection (`invulnerableTimer`).
        if flags & EF_INVULNERABLE != 0 {
            flags &= !EF_INVULNERABLE;
        }
        // A push's body effect (`g_active.c:2082-2091`).
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        if npc.mind.push_effect_time > level_time {
            flags |= EF_BODYPUSH;
        } else if npc.mind.push_effect_time != 0 {
            npc.mind.push_effect_time = 0;
            flags &= !EF_BODYPUSH;
        }
        if npc.player.stats[STAT_HOLDABLE_ITEMS] & (1 << HI_JETPACK) != 0 {
            flags |= EF_JETPACK;
        } else {
            flags &= !EF_JETPACK;
        }
        let kind = if npc.mind.noclip || flags & EF_DISINTEGRATION != 0 {
            PM_NOCLIP
        } else if npc.player.stats[STAT_HEALTH] as i32 <= 0 {
            PM_DEAD
        } else if npc.force.grip_movement_type != 0 {
            // Held by a grip (`forceGripChangeMovetype`, `g_active.c:2109-2112`): it floats.
            npc.force.grip_movement_type
        } else {
            PM_NORMAL
        };
        flags &= !(EF_JETPACK_ACTIVE | EF_JETPACK_FLAMING);
        npc.player.set_raw_field(PS_EFLAGS, flags);
        npc.player.set_movement_type(kind);
    }

    /// `ClientThink_real`'s NPC speed (`g_active.c:2192-2349`): toward the speed the
    /// command asks for, walking or running, slowed near the goal and on turns; a stop
    /// below 24; and the command's moves clamped when it walks.
    fn speed(&mut self, me: usize) {
        let npc = &mut self.actors[me];
        if npc.definition.entity_class == 53 {
            return;
        }
        let (walk, run) = (walk_speed(npc), run_speed(npc));
        let flags2 = npc.player.raw_field(PS_EFLAGS2).unwrap_or(0);
        let command = npc.mind.command;
        let mut speed;
        if !npc.mind.combat_move {
            let flying = command.up_move != 0 && flags2 & EF2_FLYING != 0;
            // Ladders are the water level's (`watertype & CONTENTS_LADDER`), which no
            // map of this game marks.
            if command.forward_move != 0 || command.right_move != 0 || flying {
                npc.mind.desired_speed = if command.buttons & BUTTON_WALKING != 0 {
                    walk
                } else {
                    run
                };
                if npc.mind.current_speed >= 80
                    && npc.mind.dist_to_goal < SLOWDOWN_DIST
                    && npc.ai_flags & NPCAI_NO_SLOWDOWN == 0
                    && npc.mind.desired_speed as f32 > MIN_NPC_SPEED
                {
                    let slow = (npc.mind.desired_speed as f32 * npc.mind.dist_to_goal
                        / SLOWDOWN_DIST)
                        .ceil();
                    npc.mind.desired_speed = slow.max(MIN_NPC_SPEED) as i32;
                }
            } else {
                npc.mind.desired_speed = 0;
            }
            accelerate(npc);
            let command = &mut npc.mind.command;
            if npc.mind.current_speed <= 24 && npc.mind.desired_speed < npc.mind.current_speed {
                npc.mind.current_speed = 0;
                speed = 0.0;
                command.forward_move = 0;
                command.right_move = 0;
            } else {
                if npc.mind.current_speed <= walk {
                    command.buttons |= BUTTON_WALKING;
                } else {
                    command.buttons &= !BUTTON_WALKING;
                }
                if npc.mind.current_speed > 0 {
                    if flying {
                        if command.up_move == 0 {
                            command.up_move = npc.mind.last_command.up_move;
                        }
                    } else if command.forward_move == 0 && command.right_move == 0 {
                        command.forward_move = npc.mind.last_command.forward_move;
                        command.right_move = npc.mind.last_command.right_move;
                    }
                }
                speed = npc.mind.current_speed as f32;
                // Slowing on turns: the yaw still to turn, in halves of a turn.
                // `(180 - fabs(...)) / 180`: `fabs` is a double's, and so is the rest.
                let turn = ((180.0
                    - f64::from(
                        crate::npc_senses::angle_delta(npc.mind.current_angles[1], npc.desired_yaw)
                            .abs(),
                    ))
                    / 180.0) as f32;
                if turn < 0.75 {
                    speed = 0.0;
                } else if npc.mind.dist_to_goal < 100.0 && turn < 1.0 {
                    speed = (f64::from(speed * turn)).floor() as f32;
                }
            }
        } else {
            npc.mind.desired_speed = if npc.mind.command.buttons & BUTTON_WALKING != 0 {
                walk
            } else {
                run
            };
            speed = npc.mind.desired_speed as f32;
        }
        let command = &mut npc.mind.command;
        if command.buttons & BUTTON_WALKING != 0 {
            command.forward_move = command.forward_move.clamp(-64, 64);
            command.right_move = command.right_move.clamp(-64, 64);
        }
        npc.player.set_speed(speed);
        npc.player.set_base_speed(speed as i32);
    }

    /// `G_CheckMovingLoopingSounds` (`g_active.c:1511-1551`): a humming droid that moves
    /// registers its hum (the entity's loop sound itself is the player state's, which the
    /// conversion copies over it).
    fn loop_sounds(&mut self, me: usize) {
        let npc = &self.actors[me];
        let command = npc.mind.command;
        let flyer = npc.player.raw_field(46).unwrap_or(0) as i32 <= 0;
        let moving = npc.mind.move_dir != [0.0; 3]
            || command.forward_move != 0
            || command.right_move != 0
            || (command.up_move != 0 && flyer)
            || (flyer && npc.player.velocity() != [0.0; 3] && npc.health > 0);
        if !moving {
            return;
        }
        if let Some((_, sound)) = HUMMING
            .iter()
            .find(|(class, _)| *class == npc.definition.client_class)
        {
            self.host.sound_index(sound);
        }
    }

    /// The move (`g_active.c:2820-3018`): `Pmove` over the NPC's state with its box,
    /// class and skeleton, the other NPCs' bodies in its way.
    fn run_move(&mut self, me: usize) {
        let Self {
            actors,
            host,
            bodies,
            body_legs,
            rider,
            passengers,
            impact_bodies,
            ..
        } = self;
        crate::vehicle_think::gather_impact_bodies(actors, me, *host, impact_bodies);
        bodies.clear();
        let number = actors[me].number;
        // The legs of the clients its move may read: the players' (the host's), the NPCs'.
        body_legs.clear();
        body_legs.extend(
            host.players()
                .iter()
                .filter_map(|player| Some((player.number, host.client_legs(player.number)?))),
        );
        body_legs.extend(
            actors
                .iter()
                .filter(|other| other.number != number)
                .map(|other| {
                    (
                        other.number,
                        other.state.raw_field(ES_LEGS_ANIM).unwrap_or(0) as u16,
                    )
                }),
        );
        // What it owns its traces pass: a vehicle's droid unit.
        bodies.extend(
            actors
                .iter()
                .filter(|other| {
                    other.number != number
                        && other.contents != 0
                        && !crate::vehicle_droid::owned_by(other, number)
                })
                .map(|other| other.body()),
        );
        let npc = &mut actors[me];
        let body = NpcBody {
            class: npc.definition.entity_class,
            mins: npc.mins,
            maxs: npc.maxs,
            humanoid: npc.humanoid,
            flags2: npc.player.raw_field(PS_EFLAGS2).unwrap_or(0),
        };
        reseed(npc);
        npc.movement.set_npc(Some(body));
        npc.movement
            .set_force_jump(npc.force.jump_charge, npc.force.jump_flip);
        let [first, second] = &npc.definition.sabers;
        let held = |saber: &crate::saber_definition::SaberDefinition, scale: f32| {
            if saber.is_held() { scale } else { 1.0 }
        };
        npc.movement.set_saber_scales(
            [
                held(first, first.anim_speed_scale),
                held(second, second.anim_speed_scale),
            ],
            [
                held(first, first.move_speed_scale),
                held(second, second.move_speed_scale),
            ],
        );
        let level = |power: usize| npc.force_levels[power].clamp(0, 255) as u8;
        let legs_of = |number: u16| {
            body_legs
                .iter()
                .find(|(known, _)| *known == number)
                .map(|(_, legs)| *legs)
        };
        let npcs = |number: u16| bodies.iter().any(|body| body.entity == number);
        let context = MoveContext {
            saber_offense: level(FP_SABER_OFFENSE),
            saber_defense: level(FP_SABER_OFFENSE + 1),
            saber_throw: level(FP_SABER_OFFENSE + 2),
            sabers: crate::saber_info::Sabers {
                first: first.is_held().then(|| first.movement()),
                second: second.is_held().then(|| second.movement()),
            },
            gametype: host.gametype(),
            saber_throws: true,
            bodies: &legs_of,
            npcs: &npcs,
            // The host that keeps the NPC's model reads its feet ([`NpcHost::move_npc`]).
            foot_bolts: &crate::pmove::no_foot_bolts,
        };
        // In a saber lock, or with its lock's frame left over ([`crate::npc_saber_lock`]).
        let in_lock = npc.player.saber_lock_time() != 0
            || npc.player.raw_field(PS_SABER_LOCK_FRAME).unwrap_or(0) != 0;
        // `ps.saberLockEnemy`, whom a lock's outcome reaches.
        let lock_enemy = npc.player.raw_field(110).unwrap_or(0) as u16;
        let mut lock_outcome = None;
        let requests = if npc.vehicle.is_some() {
            let command = npc.mind.command;
            let level_time = self.level_time;
            crate::vehicle_think::move_vehicle(
                npc,
                *host,
                command,
                &context,
                bodies,
                level_time,
                rider.as_mut(),
                passengers,
                impact_bodies,
            )
        } else if self.level.native_sand_creatures
            && crate::npc_sand_creature::is_sand_creature(npc)
            && npc.contents == 0
        {
            host.move_npc_buried(
                &mut npc.movement,
                npc.mind.command,
                &context,
                npc.number,
                bodies,
            );
            Vec::new()
        } else if in_lock {
            lock_outcome = crate::npc_saber_lock::locked_move(actors, me, *host, &context, bodies);
            Vec::new()
        } else {
            host.move_npc(
                &mut npc.movement,
                npc.mind.command,
                &context,
                npc.number,
                bodies,
            );
            Vec::new()
        };
        let npc = &mut actors[me];
        write_back(npc);
        npc.force.jump_flip = npc.movement.pending_force_jump_flip();
        npc.mind.force_jump_sound |= npc.movement.take_force_jump_sound();
        crate::vehicle_think::write_flags(npc);
        npc.movement.copied_to_entity();
        let (alive, moved_box) = (npc.health > 0, npc.movement.box_bounds());
        for request in requests {
            self.vehicle_request(me, request);
        }

        crate::vehicle_think::killed_by_its_move(&mut self.actors[me], alive, moved_box);
        if let Some(outcome) = lock_outcome {
            self.lock_outcome(me, lock_enemy, outcome);
        }
    }

    /// The conversion that ends the move (`g_active.c:3322-3345`): the `ET_NPC` entity from
    /// the player state, extrapolated from its command time; an event it could not carry
    /// sent in a temp entity of its own; the NPC where the entity is.
    fn convert(&mut self, me: usize) {
        let Self {
            actors,
            host,
            overflow,
            ..
        } = self;
        let npc = &mut actors[me];
        let sent = crate::player_entity::extrapolate(
            &npc.player,
            &mut npc.mind.shown_events,
            &mut npc.state,
            overflow,
        );
        npc.state.set_raw_field(es::TYPE, ET_NPC);
        if sent {
            host.raise_entity(overflow, npc.number);
        }
        npc.current_origin =
            es::POS_BASE.map(|index| f32::from_bits(npc.state.raw_field(index).unwrap_or(0)));
    }

    /// `NPC_SetAnim` (`NPC.c:1990-2000`, `G_SetAnim`): an animation set on the NPC's player
    /// state with its own skeleton's lengths.
    pub fn set_animation(&mut self, me: usize, parts: u8, animation: u16, flags: u8) {
        let npc = &mut self.actors[me];
        reseed(npc);
        npc.movement.set_animation_parts(parts, animation, flags);
        write_back(npc);
    }
}

/// `G_AddPushVecToUcmd` (`g_active.c:1171-1203`): another NPC's shove (`pushVec`, from
/// `NAVNEW_PushBlocker`) added to the NPC's move, its speed the sum's; the shove ends once
/// its time is past.
fn push_command(npc: &mut NpcActor, level_time: i32) {
    let push = npc.mind.tactics.push_vec;
    if push[0] * push[0] + push[1] * push[1] + push[2] * push[2] == 0.0 {
        return;
    }
    let (forward, right) = crate::pmove::flight::flight_axes(npc.player.view_angles());
    let (forward, right) = (forward.to_array(), right.to_array());
    let command = &mut npc.mind.command;
    let speed = npc.player.speed();
    let forward_speed = f32::from(command.forward_move) / 127.0 * speed;
    let right_speed = f32::from(command.right_move) / 127.0 * speed;
    let mut wish: [f32; 3] = std::array::from_fn(|axis| {
        forward[axis] * forward_speed + right_speed * right[axis] + push[axis]
    });
    npc.player
        .set_speed(crate::saber_clash::normalize(&mut wish));
    let dot = |a: [f32; 3]| f64::from(a[0] * wish[0] + a[1] * wish[1] + a[2] * wish[2]);
    command.forward_move = (127.0 * dot(forward)).floor() as i8;
    command.right_move = (127.0 * dot(right)).floor() as i8;
    if npc.mind.tactics.push_vec_time < level_time {
        npc.mind.tactics.push_vec = [0.0; 3];
    }
}

/// The movement started again from the NPC's player state, with the health stat the
/// game's whole int (the wire's is a short), and the animations its entity shows.
fn reseed(npc: &mut NpcActor) {
    npc.movement = npc.movement.reseeded(&npc.player);
    npc.movement
        .set_npc_heights(npc.definition.stand_height, npc.definition.crouch_height);
    npc.movement
        .set_health(npc.player.stats[STAT_HEALTH] as i32);
    let entity =
        [ES_LEGS_ANIM, ES_TORSO_ANIM].map(|index| npc.state.raw_field(index).unwrap_or(0) as u16);
    npc.movement.set_entity_animations(entity[0], entity[1]);
}

/// The movement's state written back into the NPC's player state. The health stat and
/// the team stay the game's whole ints: the movement keeps them as the wire's short and
/// byte (an NPC's team may be -1).
fn write_back(npc: &mut NpcActor) {
    const PERS_TEAM: usize = 3;
    let (health, team) = (
        npc.player.stats[STAT_HEALTH],
        npc.player.persistent[PERS_TEAM],
    );
    npc.movement.write_player_state(&mut npc.player);
    npc.player.stats[STAT_HEALTH] = health;
    npc.player.persistent[PERS_TEAM] = team;
}

/// A fresh movement for an NPC's player state: authoritative, with its skeleton's
/// animation lengths where the host has them.
pub fn fresh_movement(player: &sjk_protocol::PlayerState) -> Predictor {
    Predictor::from_player_state(player, movement_config())
}

/// An entity state for a temp entity, cleared.
pub fn blank_entity() -> EntityState {
    EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS)
}
