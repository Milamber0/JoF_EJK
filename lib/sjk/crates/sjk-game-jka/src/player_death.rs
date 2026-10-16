//! A player dies: `G_Kill` → `player_die` (OpenJK `codemp/game/g_cmds.c:515`,
//! `g_combat.c:2092-2918`) for a human player killing itself outside team games, duels
//! and siege — what it does to the wire state and the entity, what it tells everyone
//! (the saber's off sound, a muted weapon channel, the obituary), what it tells the dead
//! player (the scoreboard), and when it may respawn. Held against `tools/game-oracle/death.c`.
//!
//! The death animation is drawn (`G_PickDeathAnim`, `BG_PickAnim`) with the reference's
//! own generator, ported as [`Rng`]: a server seeds it as it likes, the oracle with a
//! known seed so that the draw is reproducible. Deaths from damage, dismemberment, the
//! Jedi Master, flags and NPCs are not here.

use crate::event_entity::EventEntity;
use crate::pmove::MovementState;
use crate::pmove_anim::{
    AnimationLengths, SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_FLAG_RESTART,
    set_animation,
};
use sjk_protocol::{EntityState, PlayerState};

/// `holdrand`: the game module's own generator (`q_math.c:223-260`), a VC libc `rand`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rng(pub u32);

impl Rng {
    /// `Q_irand`: an integer in `min..=max`. The C arithmetic wraps where a range wider
    /// than `QRAND_MAX` overflows it (an NPC with a vast health's `Q_irand(0, health)`).
    pub fn irand(&mut self, min: i32, max: i32) -> i32 {
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        let result = (self.0 >> 17) as i32;
        (result.wrapping_mul(max.wrapping_add(1).wrapping_sub(min)) >> 15).wrapping_add(min)
    }

    /// `Q_flrand` (`q_math.c:231-245`): a float in `min..max`, from the same draw.
    pub fn flrand(&mut self, min: f32, max: f32) -> f32 {
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        let result = (self.0 >> 17) as f32;
        ((result * (max - min)) / 32_768.0) + min
    }
}

/// `meansOfDeath_t::MOD_SUICIDE`.
pub const MOD_SUICIDE: u32 = 39;
/// `CARNAGE_REWARD_TIME`.
const CARNAGE_REWARD_TIME: i32 = 3_000;
const MAX_CLIENTS: u16 = 32;
const ENTITY_WORLD: u16 = 1_022;
const EV_DEATH1: u32 = 90;
const EV_GENERAL_SOUND: u32 = 76;
const EV_MUTE_SOUND: u32 = 74;
const EV_OBITUARY: u32 = 93;
const CHAN_WEAPON: u32 = 2;
const PM_DEAD: u8 = 5;
const WP_SABER: u32 = 3;
const GIB_HEALTH: i32 = -40;
const CONTENTS_CORPSE: u32 = 0x200;
const PMF_STUCK_TO_WALL: u32 = 16_384;
const STAT_HEALTH: usize = 0;
const STAT_HOLDABLE_ITEMS: usize = 2;
const STAT_HOLDABLE_ITEM: usize = 1;
const PERS_SCORE: usize = 0;
const PERS_KILLED: usize = 8;
/// `PERS_PLAYEREVENTS`, `PLAYEREVENT_GAUNTLETREWARD`, `MOD_STUN_BATON`.
const PERS_PLAYEREVENTS: usize = 5;
const PLAYEREVENT_GAUNTLETREWARD: u32 = 0x2;
const MOD_STUN_BATON: u32 = 1;
const PS_EXTERNAL_EVENT: usize = 56;
const PS_EXTERNAL_EVENT_PARM: usize = 64;
const PS_PM_TYPE: usize = 63;
const PS_PM_FLAGS: usize = 38;
const PS_ROCKET_LOCK_TIME: usize = 79;
const PS_ZOOM_MODE: usize = 90;
const PS_EMPLACED_INDEX: usize = 112;
const PS_JEDI_MASTER: usize = 114;
/// The entity's `isJediMaster`.
const ES_JEDI_MASTER: usize = 97;
const PS_SABER_HOLSTERED: usize = 81;
const PS_SABER_IN_FLIGHT: usize = 88;
const PS_SABER_ENTITY: usize = 31;
const PS_WEAPON: usize = 47;
const PS_LEGS_ANIM: usize = 13;
const PS_TORSO_ANIM: usize = 15;
const PS_TORSO_TIMER: usize = 20;
const PS_LEGS_TIMER: usize = 21;
const PS_TORSO_FLIP: usize = 55;
const PS_LEGS_FLIP: usize = 69;
const ES_WEAPON: usize = 14;
const ES_POWERUPS: usize = 77;
const ES_BOLT2: usize = 63;
const ES_LOOP_SOUND: usize = 55;
/// `s.loopIsSoundset` (`msg.cpp`'s entity field 70).
const ES_LOOP_IS_SOUNDSET: usize = 70;
const ES_ANGLES: [usize; 3] = [25, 9, 24];
/// `EV_EVENT_BITS`.
const EVENT_BITS: u32 = 0x300;
const EVENT_BIT1: u32 = 0x100;

/// What the game keeps between deaths: the player's respawn time, and `player_die`'s
/// static counter — which of the three death events comes next, shared by every death on
/// the server (the caller keeps one per server and copies it in and out).
#[derive(Clone, Copy, Debug, Default)]
pub struct Mortality {
    /// `client->respawnTime`: no respawn before this server time.
    pub respawn_time: i32,
    /// `client->lastKillTime`: when this player last killed somebody, for the excellent
    /// award.
    pub last_kill_time: i32,
}

/// What every death on the server shares: the game's generator (`Rand_Init`), for the
/// death animations it draws, and `player_die`'s static counter that cycles the death
/// event through `EV_DEATH1..3`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Deaths {
    pub rng: Rng,
    pub next_death_event: u8,
}

/// What a death needs to know beyond the player's state.
#[derive(Clone, Copy, Debug)]
pub struct DeathRequest {
    /// `level.time`.
    pub level_time: i32,
    /// The player's wire client number.
    pub client: u16,
    /// `r.currentOrigin`: where the sounds and the obituary are.
    pub origin: [f32; 3],
    /// Each saber's off sound (`saber[n].soundOff`), zero for none; the second's only
    /// where a second saber is held.
    pub saber_off_sounds: [u16; 2],
    /// The health the caller set before `player_die`: `Cmd_Kill_f` -999, `SetTeam` 0,
    /// `G_Damage` what the blow left.
    pub health: i32,
    /// The attacker's wire client number; `None` for the world, the player's own for a
    /// suicide.
    pub attacker: Option<u16>,
    /// The attacker is an NPC (a client, `attacker->client`, that is no player): its kill
    /// costs the dead nothing, and its point is the NPCs' own ([`crate::npc_death`]).
    pub npc_attacker: bool,
    /// `meansOfDeath`.
    pub means: u32,
    /// The killing blow's damage (`Cmd_Kill_f` says 100000) and where it landed
    /// (`pos1`, zero when nothing ever hit the player) — the death animation's choice.
    pub damage: i32,
    pub point: [f32; 3],
    /// The linked box, grown by a unit (`r.absmin`, `r.absmax`), for where the blow
    /// landed on the body.
    pub bounds: ([f32; 3], [f32; 3]),
    /// The killer's `lastKillTime`, when the attacker is another player.
    pub killer_last_kill_time: i32,
    /// `level.gametype`: what `TossClientItems` drops.
    pub gametype: i32,
    /// `pers.cmd.weapon`: the weapon being switched to, which a player putting the
    /// pistol away drops.
    pub command_weapon: u8,
    /// `gGAvoidDismember`: the finishing blow of a lost saber lock, which dies in its
    /// own pose.
    pub avoid_dismember: bool,
    /// In a duel, the other of the two sorted first (`level.sortedClients`): a suicide or
    /// a death by the world is its point (`g_combat.c:2522-2546, 2619-2641`).
    pub duel_opponent: Option<u16>,
    /// In a Jedi Master game, the masters as the scoring reads them; `None` in any other.
    pub jedi_master: Option<crate::jedi_master::Masters>,
    /// The player is in space (`client->inSpaceIndex`, [`crate::vehicle_triggers`]): it
    /// dies choking (`BOTH_CHOKE3`).
    pub in_space: bool,
}

impl DeathRequest {
    /// `Cmd_Kill_f`'s and `SetTeam`'s: a suicide with no point of impact.
    pub fn suicide(
        level_time: i32,
        client: u16,
        origin: [f32; 3],
        saber_off_sounds: [u16; 2],
        health: i32,
    ) -> Self {
        Self {
            level_time,
            client,
            origin,
            saber_off_sounds,
            health,
            attacker: Some(client),
            npc_attacker: false,
            means: MOD_SUICIDE,
            damage: 100_000,
            point: [0.0; 3],
            bounds: ([0.0; 3], [0.0; 3]),
            killer_last_kill_time: 0,
            gametype: 0,
            command_weapon: 0,
            avoid_dismember: false,
            duel_opponent: None,
            jedi_master: None,
            in_space: false,
        }
    }
}

/// What a death leaves for the server to do.
#[derive(Clone, Debug, PartialEq)]
pub struct Died {
    /// The event entities, in order: the saber going off, the weapon channel muted, the
    /// obituary, the dropped weapon's model taken off the body.
    pub events: Vec<EventEntity>,
    /// The items the player dropped, spawned after the events in this order.
    pub dropped: Vec<crate::items::Pickup>,
    /// The entity's `health` — the caller's, lifted to `GIB_HEALTH + 1` from gib level —
    /// which `ClientEndFrame` copies to the stat one frame on (the frame in which a
    /// `kill`'s corpse is `ET_INVISIBLE`).
    pub entity_health: i32,
    /// `r.contents` and `r.maxs[2]`: a corpse, eight units below its origin.
    pub contents: u32,
    pub top: f32,
    /// `AddScore` for the killer: who, and what it gains — a point for a foe, one lost
    /// for a suicide or a teammate — with `PERS_EXCELLENT_COUNT` stepped for a second
    /// kill within `CARNAGE_REWARD_TIME`.
    pub killer_score: Option<KillerScore>,
    /// `Cmd_Score_f`: the dying player is shown the scoreboard.
    pub show_scoreboard: bool,
    /// Where `TossClientItems`' events begin in [`Self::events`]: what comes between
    /// (`Team_FragBonuses`, a flag sent home) is the caller's.
    pub tossed_from: usize,
    /// The flags the player carried as it died (`powerups[PW_REDFLAG]`,
    /// `[PW_BLUEFLAG]`), which `Team_FragBonuses` reads.
    pub carried_flags: [u32; 2],
    /// The flag a suicide or a fall to death sends home rather than dropping it
    /// (`Team_ReturnFlag`): its powerup, `PW_REDFLAG` or `PW_BLUEFLAG`.
    pub returned_flag: Option<usize>,
    /// In a Jedi Master game, the master who gains the point for a death between two
    /// others (`AddScore(G_GetJediMaster(), +1)`).
    pub master_point: Option<u16>,
    /// The dead was the master and lost the saber (`ThrowSaberToAttacker`): towards its
    /// killer, or home (`None`). `ps.isJediMaster` is already cleared.
    pub saber_lost: Option<Option<u16>>,
    /// The death animation `G_PickDeathAnim` gave, where it gave one: a blade's kill then
    /// may cut a limb off (`G_CheckForDismemberment`, [`crate::npc_dismember_check`]).
    pub death_animation: Option<u16>,
}

/// What a kill is worth to the killer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KillerScore {
    pub killer: u16,
    pub points: i32,
    pub excellent: bool,
    /// A kill with the stun baton: `PERS_GAUNTLET_FRAG_COUNT` on the killer (and the
    /// humiliation flipped on the dead, `PERS_PLAYEREVENTS`).
    pub gauntlet: bool,
    /// A kill, which the killer remembers (`lastKillTime`); not a duel's point given
    /// for the other's suicide, which is `AddScore` alone.
    pub kill: bool,
}

/// `body_die` (`g_combat.c:700-757`) for a player's corpse hurt again (`G_Damage` on a
/// target already dead calls its `die`, which `player_die` left as this): the health
/// lifted back to `GIB_HEALTH + 1` from below it, and — unless the corpse is fresher
/// than two seconds past its `respawnTime`, or already disintegrated — the corpse
/// disintegrated where it lies (`EF_DISINTEGRATION`, `lastHitLoc` its origin). Returns
/// whether it was.
pub fn corpse_hit(
    state: &mut PlayerState,
    health: &mut i32,
    respawn_time: i32,
    level_time: i32,
) -> bool {
    const EF_DISINTEGRATION: u32 = 1 << 26;
    const PS_EFLAGS: usize = 17;
    let mut disintegrate = false;
    if *health < GIB_HEALTH + 1 {
        *health = GIB_HEALTH + 1;
        state.stats[STAT_HEALTH] = *health as u32;
        disintegrate = level_time - respawn_time >= 2_000;
    }
    let flags = state.raw_field(PS_EFLAGS).unwrap_or(0);
    if flags & EF_DISINTEGRATION != 0 || !disintegrate {
        return false;
    }
    let origin = state.origin();
    crate::disruptor::disintegrated(
        state,
        origin,
        (
            state.raw_field(13).unwrap_or(0),
            state.raw_field(15).unwrap_or(0),
        ),
    );
    true
}

/// `PW_REDFLAG`, `PW_BLUEFLAG`.
const PW_REDFLAG: usize = 4;
const PW_BLUEFLAG: usize = 5;

/// `player_die` (`g_combat.c:2092-2918`) for a player killed by a player or the world:
/// `state` and `entity` are the player's, `shared` its game memory, `lengths` the
/// animation table; returns what to do. The ranks (`AddScore` → `CalculateRanks`) and the
/// scoreboard are the caller's, with its view of every client. A corpse cannot die
/// again: `PM_DEAD` returns at once.
pub fn kill(
    state: &mut PlayerState,
    entity: &mut EntityState,
    shared: &mut Mortality,
    deaths: &mut Deaths,
    request: DeathRequest,
    lengths: &dyn AnimationLengths,
) -> Died {
    let read = |state: &PlayerState, index: usize| state.raw_field(index).unwrap_or(0);
    let rng = &mut deaths.rng;
    let killer = request
        .attacker
        .filter(|attacker| *attacker < MAX_CLIENTS)
        .unwrap_or(ENTITY_WORLD);
    // `player_die` never writes the health stat: its callers set it with the health
    // (`Cmd_Kill_f` -999, `SetTeam` 0) or, in `G_Damage`, wrote it from the health
    // before the entity's own floor of -999 (`g_combat.c:5398-5400, 5483-5484`), so a
    // blow of thousands leaves the stat at what it really took.
    state.set_raw_field(PS_EMPLACED_INDEX, 0);
    let mut events = Vec::with_capacity(3);
    // The sabers go off, audibly, if they were out: the first unless it is thrown.
    if read(state, PS_WEAPON) == WP_SABER
        && read(state, PS_SABER_HOLSTERED) == 0
        && read(state, PS_SABER_ENTITY) != 0
    {
        let [first, second] = request.saber_off_sounds;
        let first = (read(state, PS_SABER_IN_FLIGHT) == 0).then_some(first);
        for sound in [first, Some(second)]
            .into_iter()
            .flatten()
            .filter(|sound| *sound != 0)
        {
            events.push(EventEntity {
                event: EV_GENERAL_SOUND,
                parameter: u32::from(sound),
                origin: request.origin,
                client: None,
                broadcast: false,
                extra: [(0, 0); 12],
            });
        }
    }
    // `G_MuteSound(self->s.number, CHAN_WEAPON)`.
    events.push(
        EventEntity {
            event: EV_MUTE_SOUND,
            parameter: 0,
            origin: [0.0; 3],
            client: None,
            broadcast: true,
            extra: [(0, 0); 12],
        }
        .muting(request.client, CHAN_WEAPON),
    );
    state.set_raw_field(PS_PM_TYPE, u32::from(PM_DEAD));
    state.set_raw_field(PS_PM_FLAGS, read(state, PS_PM_FLAGS) & !PMF_STUCK_TO_WALL);
    // `BG_ClearRocketLock`.
    // `rocketLockTime` is a float on the wire.
    state.set_raw_field(PS_ROCKET_LOCK_TIME, (-1.0_f32).to_bits());
    // The obituary, then the score: a death is counted; a suicide or a death by the
    // world costs the dead a point, a kill earns the killer one (and a second within three
    // seconds of the last, the excellent award).
    let was_master = crate::jedi_master::is_master(state);
    let mut obituary = EventEntity {
        event: EV_OBITUARY,
        parameter: request.means,
        origin: request.origin,
        client: None,
        broadcast: false,
        extra: [(0, 0); 12],
    }
    .naming(request.client, killer);
    // The obituary says whether the dead was the master (`s.isJediMaster`).
    obituary.extra[2] = (ES_JEDI_MASTER, u32::from(was_master));
    events.push(obituary);
    state.persistent[PERS_KILLED] = state.persistent[PERS_KILLED].wrapping_add(1);
    // "in duel, if you kill yourself, the person you are dueling against gets a kill for
    // it" — a point, and nothing lost.
    let duel_point = |other: Option<u16>| {
        other
            .filter(|other| *other != request.client)
            .map(|other| KillerScore {
                killer: other,
                points: 1,
                excellent: false,
                gauntlet: false,
                kill: false,
            })
    };
    // A master who dies loses the saber: towards another player who killed it, else home.
    let (mut master_point, mut saber_lost) = (None, None);
    let npc_killer = request.attacker.filter(|_| request.npc_attacker);
    let killer_score = match request
        .attacker
        .filter(|attacker| *attacker < MAX_CLIENTS || npc_killer.is_some())
    {
        // An NPC's kill (`g_combat.c:2515-2600`, `attacker->client` and never on a
        // player's team): the dead loses nothing, the stun baton humiliates it all the
        // same, a master's saber goes towards the NPC; the NPC's point is the roster's.
        Some(npc) if npc_killer.is_some() => {
            if request.means == MOD_STUN_BATON {
                state.persistent[PERS_PLAYEREVENTS] ^= PLAYEREVENT_GAUNTLETREWARD;
            }
            if request.jedi_master.is_some() && was_master {
                saber_lost = Some(Some(npc));
            }
            None
        }
        Some(attacker) if attacker == request.client => {
            duel_point(request.duel_opponent).or_else(|| {
                state.persistent[PERS_SCORE] = state.persistent[PERS_SCORE].wrapping_sub(1);
                if request.jedi_master.is_some() && was_master {
                    saber_lost = Some(None);
                }
                Some(KillerScore {
                    killer: attacker,
                    points: -1,
                    excellent: false,
                    gauntlet: false,
                    kill: false,
                })
            })
        }
        Some(attacker) => {
            let excellent =
                request.level_time - request.killer_last_kill_time < CARNAGE_REWARD_TIME;
            // The humiliation of a stun-baton kill, on both.
            let gauntlet = request.means == MOD_STUN_BATON;
            if gauntlet {
                state.persistent[PERS_PLAYEREVENTS] ^= PLAYEREVENT_GAUNTLETREWARD;
            }
            // In Jedi Master only the master scores or is scored on; a death between two
            // others is the master's point.
            let points = match request.jedi_master {
                Some(masters) => {
                    let (points, master) = crate::jedi_master::scoring(
                        masters.killer_is_master,
                        was_master,
                        masters.master,
                    );
                    master_point = master;
                    if was_master {
                        saber_lost = Some(Some(attacker));
                    }
                    points
                }
                None => 1,
            };
            Some(KillerScore {
                killer: attacker,
                points,
                excellent,
                gauntlet,
                kill: true,
            })
        }
        // Killed by nobody (the world): the dead loses a point, as for a suicide.
        None => {
            if was_master {
                saber_lost = Some(None);
            }
            duel_point(request.duel_opponent).or_else(|| {
                state.persistent[PERS_SCORE] = state.persistent[PERS_SCORE].wrapping_sub(1);
                None
            })
        }
    };
    if saber_lost.is_some() {
        state.set_raw_field(PS_JEDI_MASTER, 0);
    }
    // A suicide or a fall to death does not drop the flag: it goes home
    // (`g_combat.c:2654-2688`).
    let carried_flags = [state.powerups[PW_REDFLAG], state.powerups[PW_BLUEFLAG]];
    let falling = read(state, crate::triggers::PS_FALLING_TO_DEATH) != 0;
    let mut returned_flag = None;
    if request.means == crate::means_of_death::MOD_SUICIDE || falling {
        returned_flag = [PW_REDFLAG, PW_BLUEFLAG]
            .into_iter()
            .find(|flag| state.powerups[*flag] != 0);
        if let Some(flag) = returned_flag {
            state.powerups[flag] = 0;
        }
    }
    let tossed_from = events.len();
    // `TossClientItems` (`g_combat.c:2670-2675`), unless the player is falling to its
    // death: the weapon in hand remembered on the entity, and dropped with the powerups.
    let tossed = if !falling {
        crate::dropped_items::toss_client_items(
            state,
            entity,
            request.command_weapon,
            request.gametype,
            request.level_time,
            rng,
        )
    } else {
        crate::dropped_items::Tossed::default()
    };
    if let Some(bolt2) = tossed.bolt2 {
        entity.set_raw_field(ES_BOLT2, bolt2);
    }
    events.extend(tossed.event);
    // What the body keeps and loses.
    entity.set_raw_field(ES_WEAPON, 0);
    entity.set_raw_field(ES_POWERUPS, 0);
    state.set_raw_field(PS_ZOOM_MODE, 0);
    // (The reference's levelling of the entity's angles and the view is commented out:
    // the dead keep looking where they looked.)
    entity.set_raw_field(ES_LOOP_SOUND, 0);
    entity.set_raw_field(ES_LOOP_IS_SOUNDSET, 0);
    state.powerups = [0; 16];
    state.stats[STAT_HOLDABLE_ITEMS] = 0;
    state.stats[STAT_HOLDABLE_ITEM] = 0;
    // The death animation, by where the blow landed and how hard (`G_PickDeathAnim`);
    // held on both halves from a normal movement type.
    // A player already in a death pose keeps it, and waits the longer respawn.
    shared.respawn_time = request.level_time + 1_700;
    let death_animation = crate::death_animation::pick(rng, lengths, state, &request, 0.0);
    if let Some(animation) = death_animation {
        let mut movement = MovementState::from_player_state(state);
        movement.movement_type = 0;
        set_animation(
            &mut movement,
            SETANIM_BOTH,
            animation,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD | SETANIM_FLAG_RESTART,
            lengths,
        );
        for (index, value) in [
            (PS_LEGS_ANIM, u32::from(movement.legs_anim)),
            (PS_TORSO_ANIM, u32::from(movement.torso_anim)),
            (PS_LEGS_TIMER, movement.legs_timer as u32),
            (PS_TORSO_TIMER, movement.torso_timer as u32),
            (PS_LEGS_FLIP, u32::from(movement.legs_flip)),
            (PS_TORSO_FLIP, u32::from(movement.torso_flip)),
        ] {
            state.set_raw_field(index, value);
        }
        shared.respawn_time = request.level_time + 1_000;
    }
    // `G_AddEvent(self, EV_DEATH1 + deathAnim, wasJediMaster)`: the death event's sequence
    // bits step on from the last external event's.
    let bits = (read(state, PS_EXTERNAL_EVENT) & EVENT_BITS).wrapping_add(EVENT_BIT1) & EVENT_BITS;
    state.set_raw_field(
        PS_EXTERNAL_EVENT,
        (EV_DEATH1 + u32::from(deaths.next_death_event)) | bits,
    );
    state.set_raw_field(PS_EXTERNAL_EVENT_PARM, u32::from(was_master));
    deaths.next_death_event = (deaths.next_death_event + 1) % 3;
    let entity_health = if request.health <= GIB_HEALTH {
        GIB_HEALTH + 1
    } else {
        request.health
    };
    Died {
        events,
        dropped: tossed.items,
        entity_health,
        contents: CONTENTS_CORPSE,
        top: -8.0,
        killer_score,
        show_scoreboard: true,
        tossed_from,
        carried_flags,
        returned_flag,
        master_point,
        saber_lost,
        death_animation,
    }
}

impl EventEntity {
    /// `G_MuteSound`'s fields: the channel in `trickedentindex`, the entity in
    /// `trickedentindex2`.
    pub(crate) fn muting(mut self, entity: u16, channel: u32) -> Self {
        self.extra = [
            (58, channel),
            (74, u32::from(entity)),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
        ];
        self
    }

    /// The obituary's fields: the victim in `otherEntityNum`, the killer in
    /// `otherEntityNum2`.
    fn naming(mut self, victim: u16, killer: u16) -> Self {
        self.extra = [
            (59, u32::from(victim)),
            (39, u32::from(killer)),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
        ];
        self
    }
}

/// `CopyToBodyQue` (`g_client.c:1051-1180`) with `ClientRespawn`'s gate
/// (`:1196-1213`): the body left behind when a dead player respawns — a copy of its
/// entity, levelled, marked dead and made a corpse, holding the weapon it died with,
/// falling if it was in the air. Returns `None` where the reference makes no body — a
/// disintegrated player, and one that fell to death in a pit, which leaves nothing
/// behind and is told to every client as `rcg` instead (inside `CONTENTS_NODROP` is the
/// caller's to know).
pub fn body_left_behind(
    state: &PlayerState,
    entity: &EntityState,
    level_time: i32,
) -> Option<Body> {
    const EF_DISINTEGRATION: u32 = 1 << 26;
    const EF_DEAD: u32 = 1 << 1;
    const ET_BODY: u32 = 15;
    const TR_STATIONARY: u32 = 0;
    const TR_GRAVITY: u32 = 5;
    const WP_BLASTER: u32 = 5;
    const ENTITY_NONE: u32 = 1_023;
    const FORCE_LIGHTSIDE: u32 = 1;
    const ES_POS_TYPE: usize = 23;
    const ES_POS_TIME: usize = 0;
    const ES_POS_DELTA: [usize; 3] = [6, 7, 10];
    const ES_APOS_BASE_PITCH_ROLL: [usize; 2] = [5, 33];
    const ES_G2_RADIUS: usize = 38;
    const ES_TYPE: usize = 8;
    const ES_EFLAGS: usize = 19;
    const ES_ORIGIN2: [usize; 3] = [56, 60, 53];
    const ES_EVENT: usize = 28;
    const ES_GROUND: usize = 22;
    const ES_LEGS: usize = 16;
    const ES_TORSO: usize = 17;
    const ES_RGBA: [usize; 4] = [30, 35, 36, 29];
    const PS_EFLAGS: usize = 17;
    const PS_LAST_HIT: [usize; 3] = [102, 105, 100];
    const PS_RGBA: [usize; 4] = [29, 42, 45, 32];
    const PS_FORCE_SIDE: usize = 61;
    let read = |index: usize| state.raw_field(index).unwrap_or(0);
    if read(PS_EFLAGS) & EF_DISINTEGRATION != 0 || read(crate::triggers::PS_FALLING_TO_DEATH) != 0 {
        return None;
    }
    let mut body = entity.clone();
    for index in [
        ES_ANGLES[0],
        ES_ANGLES[2],
        ES_APOS_BASE_PITCH_ROLL[0],
        ES_APOS_BASE_PITCH_ROLL[1],
    ] {
        body.set_raw_field(index, 0);
    }
    body.set_raw_field(ES_G2_RADIUS, 100);
    body.set_raw_field(ES_TYPE, ET_BODY);
    body.set_raw_field(ES_EFLAGS, EF_DEAD);
    for (to, from) in ES_ORIGIN2.into_iter().zip(PS_LAST_HIT) {
        body.set_raw_field(to, read(from));
    }
    body.set_raw_field(ES_POWERUPS, 0);
    body.set_raw_field(ES_LOOP_SOUND, 0);
    body.set_raw_field(ES_LOOP_IS_SOUNDSET, 0);
    if entity.raw_field(ES_GROUND) == Some(ENTITY_NONE) {
        body.set_raw_field(ES_POS_TYPE, TR_GRAVITY);
        body.set_raw_field(ES_POS_TIME, level_time as u32);
        for (index, value) in ES_POS_DELTA.into_iter().zip(state.velocity()) {
            body.set_raw_field(index, value.to_bits());
        }
    } else {
        body.set_raw_field(ES_POS_TYPE, TR_STATIONARY);
    }
    body.set_raw_field(ES_EVENT, 0);
    let mut weapon = entity.raw_field(ES_BOLT2).unwrap_or(0);
    if weapon == WP_SABER && read(PS_SABER_IN_FLIGHT) != 0 {
        weapon = WP_BLASTER;
    }
    body.set_raw_field(ES_WEAPON, weapon);
    for index in [ES_LEGS, ES_TORSO] {
        body.set_raw_field(index, read(PS_LEGS_ANIM));
    }
    for (to, from) in ES_RGBA.into_iter().zip(PS_RGBA) {
        body.set_raw_field(to, read(from));
    }
    let light = read(PS_FORCE_SIDE) == FORCE_LIGHTSIDE;
    Some(Body {
        state: body,
        weapon,
        light,
    })
}

/// A body as [`body_left_behind`] makes it, before it has a slot.
#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    /// Its wire state, unnumbered.
    pub state: EntityState,
    /// The weapon it holds.
    pub weapon: u32,
    /// Whether the player was on the light side.
    pub light: bool,
}

impl Body {
    /// `rcg <client>` (`g_client.c:1211-1213`): what every client is told instead of
    /// `ircg` when no body was made — its limbs and ragdoll are put right without one.
    pub fn no_body_command(client: u16) -> Vec<u8> {
        format!("rcg {client}").into_bytes()
    }

    /// `ircg <client> <body> <weapon> <light>`: every client moves the ragdoll over.
    pub fn command(&self, client: u16, body: u16) -> Vec<u8> {
        format!(
            "ircg {client} {body} {} {}",
            self.weapon,
            u8::from(self.light)
        )
        .into_bytes()
    }
}
