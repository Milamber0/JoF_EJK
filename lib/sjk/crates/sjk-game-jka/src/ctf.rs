//! Capture the flag (`g_team.c`): the two flags and what touching them does
//! (`Pickup_Team`, `Team_TouchEnemyFlag`, `Team_TouchOurFlag`), a dropped flag's return
//! (`Team_DroppedFlagThink`, `Team_ResetFlag`), the flags' status for every client
//! (`Team_SetFlagStatus`, `CS_FLAGSTATUS`), the CTF messages (`PrintCTFMessage`,
//! `EV_CTFMESSAGE`) and the team sounds (`EV_GLOBAL_TEAM_SOUND`), the capture's team point
//! (`AddTeamScore`) and every bonus: the flag taken, captured, returned, the capture's
//! team and assists, the carrier fragged, the carrier and base defended
//! (`Team_FragBonuses`, `Team_CheckHurtCarrier`).
//!
//! The flags are the map's items; the caller keeps them and reaches them, and every
//! player, through a [`FlagWorld`].
//!
//! Not yet: Capture the Ysalamiri's own rules (`GT_CTY` shares these), the neutral flag
//! of one-flag CTF.

use crate::event_entity::EventEntity;
use sjk_protocol::PlayerState;

pub const GT_CTF: i32 = 8;
pub const GT_CTY: i32 = 9;
pub const TEAM_RED: i32 = 1;
pub const TEAM_BLUE: i32 = 2;
const TEAM_SPECTATOR: i32 = 3;
/// `PW_REDFLAG`, `PW_BLUEFLAG`: the flag a player carries, as a powerup that never ends.
pub const PW_REDFLAG: usize = 4;
pub const PW_BLUEFLAG: usize = 5;
/// `CS_FLAGSTATUS`.
pub const CS_FLAGSTATUS: usize = 23;
const EV_GLOBAL_TEAM_SOUND: u32 = 78;
const EV_CTFMESSAGE: u32 = 99;
const ES_TRICKED: usize = 58;
const ES_TRICKED2: usize = 74;
const MAX_CLIENTS: i32 = 32;
/// `persistant[]`: `PERS_DEFEND_COUNT`, `PERS_ASSIST_COUNT`, `PERS_CAPTURES`.
const PERS_DEFEND_COUNT: usize = 11;
const PERS_ASSIST_COUNT: usize = 12;
const PERS_CAPTURES: usize = 14;

/// `flagStatus_t`: `FLAG_ATBASE`, `FLAG_TAKEN`, `FLAG_DROPPED`.
pub const FLAG_ATBASE: u8 = 0;
pub const FLAG_TAKEN: u8 = 1;
pub const FLAG_DROPPED: u8 = 4;

/// The bonuses (`g_team.h:26-44`).
const CTF_CAPTURE_BONUS: i32 = 100;
const CTF_TEAM_BONUS: i32 = 25;
const CTF_RECOVERY_BONUS: i32 = 10;
const CTF_FLAG_BONUS: i32 = 10;
const CTF_FRAG_CARRIER_BONUS: i32 = 20;
const CTF_CARRIER_DANGER_PROTECT_BONUS: i32 = 5;
const CTF_CARRIER_PROTECT_BONUS: i32 = 2;
const CTF_FLAG_DEFENSE_BONUS: i32 = 10;
const CTF_RETURN_FLAG_ASSIST_BONUS: i32 = 10;
const CTF_FRAG_CARRIER_ASSIST_BONUS: i32 = 10;
const CTF_TARGET_PROTECT_RADIUS: f32 = 1_000.0;
const CTF_ATTACKER_PROTECT_RADIUS: f32 = 1_000.0;
const CTF_CARRIER_DANGER_PROTECT_TIMEOUT: i32 = 8_000;
const CTF_FRAG_CARRIER_ASSIST_TIMEOUT: i32 = 10_000;
const CTF_RETURN_FLAG_ASSIST_TIMEOUT: i32 = 10_000;

/// `ctfMessages_t`.
const CTFMESSAGE_FRAGGED_FLAG_CARRIER: u32 = 0;
const CTFMESSAGE_FLAG_RETURNED: u32 = 1;
const CTFMESSAGE_PLAYER_RETURNED_FLAG: u32 = 2;
const CTFMESSAGE_PLAYER_CAPTURED_FLAG: u32 = 3;
const CTFMESSAGE_PLAYER_GOT_FLAG: u32 = 4;
/// `GTS_*`.
const GTS_RED_CAPTURE: u32 = 0;
const GTS_BLUE_CAPTURE: u32 = 1;
const GTS_RED_RETURN: u32 = 2;
const GTS_BLUE_RETURN: u32 = 3;
const GTS_RED_TAKEN: u32 = 4;
const GTS_BLUE_TAKEN: u32 = 5;
const GTS_REDTEAM_SCORED: u32 = 6;
const GTS_BLUETEAM_SCORED: u32 = 7;
const GTS_REDTEAM_TOOK_LEAD: u32 = 8;
const GTS_BLUETEAM_TOOK_LEAD: u32 = 9;
const GTS_TEAMS_ARE_TIED: u32 = 10;

/// `minFlagRange`, `maxFlagRange`: the box around a flag's base in which a closer rival
/// wins a touch.
const MIN_FLAG_RANGE: [f32; 3] = [50.0, 36.0, 36.0];
const MAX_FLAG_RANGE: [f32; 3] = [44.0, 36.0, 36.0];

/// The flag game (`teamgame`): each flag's status and when each was last taken, and the
/// last capture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flags {
    pub red: u8,
    pub blue: u8,
    pub red_taken_time: i32,
    pub blue_taken_time: i32,
    pub last_flag_capture: i32,
    pub last_capture_team: i32,
}

/// `ctfFlagStatusRemap`.
const STATUS_REMAP: [u8; 5] = [b'0', b'1', b'*', b'*', b'2'];

impl Flags {
    /// `CS_FLAGSTATUS`: red's status, then blue's.
    pub fn status_string(&self) -> Vec<u8> {
        vec![
            STATUS_REMAP[usize::from(self.red.min(4))],
            STATUS_REMAP[usize::from(self.blue.min(4))],
        ]
    }
}

/// A player's CTF record (`pers.teamState`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TeamState {
    pub flag_since: i32,
    pub last_hurt_carrier: i32,
    pub last_fragged_carrier: i32,
    pub last_returned_flag: i32,
    pub captures: i32,
    pub base_defense: i32,
    pub carrier_defense: i32,
    pub flag_recovery: i32,
    pub frag_carrier: i32,
    pub assists: i32,
}

/// A player as the flags reach it.
pub struct FlagPlayer<'a> {
    pub state: &'a mut PlayerState,
    pub health: i32,
    /// `sess.sessionTeam`.
    pub team: i32,
    /// `pers.connected == CON_CONNECTED`.
    pub connected: bool,
    pub team_state: &'a mut TeamState,
    /// `r.absmin`, `r.absmax`, `r.currentOrigin`.
    pub absmin: [f32; 3],
    pub absmax: [f32; 3],
    pub current_origin: [f32; 3],
}

/// Everything a flag's touch reaches.
pub trait FlagWorld {
    /// One past the highest player number.
    fn slots(&self) -> u16;
    /// A player in use (a spectator too), one at a time.
    fn player(&mut self, number: u16) -> Option<FlagPlayer<'_>>;
    /// `AddScore`: `points` to player `number` (the ranks follow once the touch is over).
    fn add_score(&mut self, number: u16, points: i32);
    /// `level.teamScores[TEAM_RED]` and `[TEAM_BLUE]`.
    fn team_scores(&mut self) -> &mut [i32; 2];
    /// A temp entity raised now.
    fn raise(&mut self, event: EventEntity);
    /// `Team_ResetFlag`'s entities: `team`'s dropped flags freed and its base flag back
    /// (`RespawnItem`). Returns the base flag's `s.pos.trBase`.
    fn reset_flag(&mut self, team: i32) -> Option<[f32; 3]>;
    /// `team`'s flag at its base (not a dropped one): its `r.currentOrigin`.
    fn base_flag(&mut self, team: i32) -> Option<[f32; 3]>;
    /// `trap->InPVS`.
    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool;
    /// `CS_FLAGSTATUS` changed to `value`.
    fn flag_status(&mut self, value: Vec<u8>);
}

/// A flag touched: whose it is, whether it lies dropped, and where it stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlagTouch {
    pub team: i32,
    pub dropped: bool,
    /// `s.pos.trBase`, `r.currentOrigin`.
    pub base: [f32; 3],
    pub current_origin: [f32; 3],
}

/// `BG_CanItemBeGrabbed` for a team item: in CTF, the other team's flag, or one's own
/// dropped (to return it), or one's own at its base while carrying the other (to
/// capture).
pub fn can_grab(gametype: i32, tag: i32, dropped: bool, state: &PlayerState) -> bool {
    if gametype != GT_CTF && gametype != GT_CTY {
        return false;
    }
    let (own, other) = match state.persistent[3] as i32 {
        TEAM_RED => (PW_REDFLAG, PW_BLUEFLAG),
        TEAM_BLUE => (PW_BLUEFLAG, PW_REDFLAG),
        _ => return false,
    };
    tag == other as i32 || (tag == own as i32 && (dropped || state.powerups[other] != 0))
}

/// `Pickup_Team`: what touching `flag` does for player `other`. Returns `Touch_Item`'s
/// respawn: 0 for nothing more (a capture, a return), -1 for a flag taken — gone from
/// where it stood, never to respawn by itself.
pub fn touch(
    flags: &mut Flags,
    world: &mut dyn FlagWorld,
    flag: FlagTouch,
    other: u16,
    level_time: i32,
    intermission_queued: bool,
) -> i32 {
    let Some(team) = world.player(other).map(|player| player.team) else {
        return 0;
    };
    if flag.team == team {
        touch_our_flag(flags, world, flag, other, level_time, intermission_queued)
    } else {
        touch_enemy_flag(flags, world, flag, other, level_time, intermission_queued)
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3)
        .map(|axis| (a[axis] - b[axis]) * (a[axis] - b[axis]))
        .sum::<f32>()
        .sqrt()
}

fn length(v: [f32; 3]) -> f32 {
    v.iter().map(|axis| axis * axis).sum::<f32>().sqrt()
}

/// The players in the box around `base` (`EntitiesInBox`), in number order.
fn near_base(world: &mut dyn FlagWorld, base: [f32; 3], out: &mut Vec<u16>) {
    let (mins, maxs): ([f32; 3], [f32; 3]) = (
        std::array::from_fn(|axis| base[axis] - MIN_FLAG_RANGE[axis]),
        std::array::from_fn(|axis| base[axis] + MAX_FLAG_RANGE[axis]),
    );
    for number in 0..world.slots() {
        if let Some(player) = world.player(number)
            && (0..3)
                .all(|axis| player.absmin[axis] <= maxs[axis] && player.absmax[axis] >= mins[axis])
        {
            out.push(number);
        }
    }
}

/// `Team_TouchOurFlag`: a dropped flag of one's own is returned; at its base, carrying
/// the other flag, it is a capture — unless a rival nearer the base takes the flag
/// first.
fn touch_our_flag(
    flags: &mut Flags,
    world: &mut dyn FlagWorld,
    flag: FlagTouch,
    other: u16,
    level_time: i32,
    intermission_queued: bool,
) -> i32 {
    let team = flag.team;
    let Some((own_team, origin)) = world
        .player(other)
        .map(|player| (player.team, player.state.origin()))
    else {
        return 0;
    };
    let enemy_flag = if own_team == TEAM_RED {
        PW_BLUEFLAG
    } else {
        PW_REDFLAG
    };
    if flag.dropped {
        print_ctf_message(
            world,
            i32::from(other),
            team,
            CTFMESSAGE_PLAYER_RETURNED_FLAG,
        );
        world.add_score(other, CTF_RECOVERY_BONUS);
        if let Some(player) = world.player(other) {
            player.team_state.flag_recovery += 1;
            player.team_state.last_returned_flag = level_time;
        }
        let base = reset_flag(flags, world, team);
        return_flag_sound(world, base, team);
        return 0;
    }
    if world
        .player(other)
        .is_none_or(|player| player.state.powerups[enemy_flag] == 0)
        || intermission_queued
    {
        return 0;
    }
    // A rival nearer the base takes the flag first.
    let near = distance(flag.base, origin);
    let enemy_team = if own_team == TEAM_RED {
        TEAM_BLUE
    } else {
        TEAM_RED
    };
    let mut listed = Vec::new();
    near_base(world, flag.base, &mut listed);
    for number in listed {
        let Some(enemy) = world.player(number) else {
            continue;
        };
        if !enemy.connected
            || enemy.health < 1
            || enemy.team == TEAM_SPECTATOR
            || enemy.team != enemy_team
        {
            continue;
        }
        if distance(flag.base, enemy.state.origin()) < near {
            return touch_enemy_flag(flags, world, flag, number, level_time, intermission_queued);
        }
    }
    print_ctf_message(
        world,
        i32::from(other),
        team,
        CTFMESSAGE_PLAYER_CAPTURED_FLAG,
    );
    if let Some(player) = world.player(other) {
        player.state.powerups[enemy_flag] = 0;
    }
    flags.last_flag_capture = level_time;
    flags.last_capture_team = team;
    add_team_score(world, flag.base, own_team, 1);
    if let Some(player) = world.player(other) {
        player.team_state.captures += 1;
        player.state.persistent[PERS_CAPTURES] =
            player.state.persistent[PERS_CAPTURES].wrapping_add(1);
    }
    world.add_score(other, CTF_CAPTURE_BONUS);
    capture_flag_sound(world, flag.base, team);
    // The bonuses: the capture's team, and its assists.
    let mut assists = 0;
    for number in 0..world.slots() {
        if number == other {
            continue;
        }
        let Some(player) = world.player(number) else {
            continue;
        };
        if player.team != own_team {
            player.team_state.last_hurt_carrier = -5;
            continue;
        }
        let (returned, fragged) = (
            player.team_state.last_returned_flag,
            player.team_state.last_fragged_carrier,
        );
        world.add_score(number, CTF_TEAM_BONUS);
        for (last, bonus, timeout) in [
            (
                returned,
                CTF_RETURN_FLAG_ASSIST_BONUS,
                CTF_RETURN_FLAG_ASSIST_TIMEOUT,
            ),
            (
                fragged,
                CTF_FRAG_CARRIER_ASSIST_BONUS,
                CTF_FRAG_CARRIER_ASSIST_TIMEOUT,
            ),
        ] {
            if last + timeout > level_time {
                world.add_score(number, bonus);
                assists += 1;
                if let Some(player) = world.player(number) {
                    player.state.persistent[PERS_ASSIST_COUNT] =
                        player.state.persistent[PERS_ASSIST_COUNT].wrapping_add(1);
                }
            }
        }
    }
    if let Some(player) = world.player(other) {
        player.team_state.assists += assists;
    }
    reset_flags(flags, world);
    0
}

/// `Team_TouchEnemyFlag`: the other team's flag taken — unless one of its own team,
/// carrying this player's flag, stands nearer and so captures first.
fn touch_enemy_flag(
    flags: &mut Flags,
    world: &mut dyn FlagWorld,
    flag: FlagTouch,
    other: u16,
    level_time: i32,
    intermission_queued: bool,
) -> i32 {
    let team = flag.team;
    let Some((own_team, origin)) = world
        .player(other)
        .map(|player| (player.team, player.state.origin()))
    else {
        return 0;
    };
    let near = distance(flag.base, origin);
    let our_flag = if own_team == TEAM_RED {
        PW_REDFLAG
    } else {
        PW_BLUEFLAG
    };
    let mut listed = Vec::new();
    near_base(world, flag.base, &mut listed);
    for number in listed {
        let Some(enemy) = world.player(number) else {
            continue;
        };
        if enemy.team == TEAM_SPECTATOR || enemy.health < 1 || enemy.state.powerups[our_flag] == 0 {
            continue;
        }
        if distance(flag.base, enemy.state.origin()) < near {
            return touch_our_flag(flags, world, flag, number, level_time, intermission_queued);
        }
    }
    print_ctf_message(world, i32::from(other), team, CTFMESSAGE_PLAYER_GOT_FLAG);
    if let Some(player) = world.player(other) {
        player.state.powerups[if team == TEAM_RED {
            PW_REDFLAG
        } else {
            PW_BLUEFLAG
        }] = i32::MAX as u32;
    }
    set_flag_status(flags, world, team, FLAG_TAKEN);
    world.add_score(other, CTF_FLAG_BONUS);
    if let Some(player) = world.player(other) {
        player.team_state.flag_since = level_time;
    }
    take_flag_sound(flags, world, flag.base, team, level_time);
    -1
}

/// `Team_SetFlagStatus`: a changed status is published.
pub fn set_flag_status(flags: &mut Flags, world: &mut dyn FlagWorld, team: i32, status: u8) {
    let slot = match team {
        TEAM_RED => &mut flags.red,
        TEAM_BLUE => &mut flags.blue,
        _ => return,
    };
    if *slot != status {
        *slot = status;
        world.flag_status(flags.status_string());
    }
}

/// `Team_ResetFlag`: `team`'s flag home, its status at base. Returns the base flag's
/// `trBase`.
pub fn reset_flag(flags: &mut Flags, world: &mut dyn FlagWorld, team: i32) -> Option<[f32; 3]> {
    let base = world.reset_flag(team);
    set_flag_status(flags, world, team, FLAG_ATBASE);
    base
}

/// `Team_ResetFlags`: both flags home.
pub fn reset_flags(flags: &mut Flags, world: &mut dyn FlagWorld) {
    reset_flag(flags, world, TEAM_RED);
    reset_flag(flags, world, TEAM_BLUE);
}

/// `Team_DroppedFlagThink`: a dropped flag's thirty seconds are up, and it goes home.
pub fn dropped_flag_expired(flags: &mut Flags, world: &mut dyn FlagWorld, team: i32) {
    let base = reset_flag(flags, world, team);
    return_flag_sound(world, base, team);
}

/// `Team_ReturnFlag`: a flag sent home — lost where nothing may lie (`Team_FreeEntity`),
/// or carried by one who killed itself or fell — and everyone told.
pub fn return_flag(flags: &mut Flags, world: &mut dyn FlagWorld, team: i32) {
    let base = reset_flag(flags, world, team);
    return_flag_sound(world, base, team);
    print_ctf_message(world, -1, team, CTFMESSAGE_FLAG_RETURNED);
}

/// `Team_CheckDroppedItem`: a flag dropped is `FLAG_DROPPED`.
pub fn flag_dropped(flags: &mut Flags, world: &mut dyn FlagWorld, tag: i32) {
    match tag as usize {
        PW_REDFLAG => set_flag_status(flags, world, TEAM_RED, FLAG_DROPPED),
        PW_BLUEFLAG => set_flag_status(flags, world, TEAM_BLUE, FLAG_DROPPED),
        _ => {}
    }
}

/// A team's flag, from a flag item's powerup tag.
pub fn flag_team(tag: i32) -> Option<i32> {
    match tag as usize {
        PW_REDFLAG => Some(TEAM_RED),
        PW_BLUEFLAG => Some(TEAM_BLUE),
        _ => None,
    }
}

/// `PrintCTFMessage`: everyone told, player and team named (a capture names the team
/// that lost the flag's rival).
fn print_ctf_message(world: &mut dyn FlagWorld, player: i32, team: i32, message: u32) {
    let player = if player == -1 {
        MAX_CLIENTS + 1
    } else {
        player
    };
    let named = if message == CTFMESSAGE_PLAYER_CAPTURED_FLAG {
        if team == TEAM_RED {
            TEAM_BLUE
        } else {
            TEAM_RED
        }
    } else if team == -1 {
        50
    } else {
        team
    };
    let mut event = EventEntity {
        event: EV_CTFMESSAGE,
        parameter: message,
        origin: [0.0; 3],
        client: None,
        broadcast: true,
        extra: [(0, 0); 12],
    };
    event.extra[0] = (ES_TRICKED, player as u32);
    event.extra[1] = (ES_TRICKED2, named as u32);
    world.raise(event);
}

/// A team sound at `origin`, to everyone.
fn team_sound(world: &mut dyn FlagWorld, origin: [f32; 3], sound: u32) {
    world.raise(EventEntity {
        event: EV_GLOBAL_TEAM_SOUND,
        parameter: sound,
        origin,
        client: None,
        broadcast: true,
        extra: [(0, 0); 12],
    });
}

/// `AddTeamScore`: the team's points, with the sound of the scoring, the lead taken, or
/// the teams tied.
fn add_team_score(world: &mut dyn FlagWorld, origin: [f32; 3], team: i32, score: i32) {
    let [red, blue] = *world.team_scores();
    let (ours, theirs) = if team == TEAM_RED {
        (red, blue)
    } else {
        (blue, red)
    };
    let sound = if ours + score == theirs {
        GTS_TEAMS_ARE_TIED
    } else if ours <= theirs && ours + score > theirs {
        if team == TEAM_RED {
            GTS_REDTEAM_TOOK_LEAD
        } else {
            GTS_BLUETEAM_TOOK_LEAD
        }
    } else if team == TEAM_RED {
        GTS_REDTEAM_SCORED
    } else {
        GTS_BLUETEAM_SCORED
    };
    team_sound(world, origin, sound);
    let slot = if team == TEAM_RED { 0 } else { 1 };
    world.team_scores()[slot] += score;
}

/// `Team_ReturnFlagSound`: none without a base flag.
fn return_flag_sound(world: &mut dyn FlagWorld, base: Option<[f32; 3]>, team: i32) {
    let Some(base) = base else { return };
    team_sound(
        world,
        base,
        if team == TEAM_BLUE {
            GTS_RED_RETURN
        } else {
            GTS_BLUE_RETURN
        },
    );
}

/// `Team_TakeFlagSound`: only when the flag was at its base, or not taken in the last ten
/// seconds — which the game reads off the other flag's status.
fn take_flag_sound(
    flags: &mut Flags,
    world: &mut dyn FlagWorld,
    origin: [f32; 3],
    team: i32,
    level_time: i32,
) {
    match team {
        TEAM_RED => {
            if flags.blue != FLAG_ATBASE && flags.blue_taken_time > level_time - 10_000 {
                return;
            }
            flags.blue_taken_time = level_time;
        }
        TEAM_BLUE => {
            if flags.red != FLAG_ATBASE && flags.red_taken_time > level_time - 10_000 {
                return;
            }
            flags.red_taken_time = level_time;
        }
        _ => {}
    }
    team_sound(
        world,
        origin,
        if team == TEAM_BLUE {
            GTS_RED_TAKEN
        } else {
            GTS_BLUE_TAKEN
        },
    );
}

/// `Team_CaptureFlagSound`.
fn capture_flag_sound(world: &mut dyn FlagWorld, origin: [f32; 3], team: i32) {
    team_sound(
        world,
        origin,
        if team == TEAM_BLUE {
            GTS_BLUE_CAPTURE
        } else {
            GTS_RED_CAPTURE
        },
    );
}

/// `Team_CheckHurtCarrier`: a blow on the other team's flag carrier is remembered by the
/// one who dealt it.
pub fn check_hurt_carrier(
    target: &PlayerState,
    target_team: i32,
    attacker: &mut TeamState,
    attacker_team: i32,
    level_time: i32,
) {
    let flag = if target_team == TEAM_RED {
        PW_BLUEFLAG
    } else {
        PW_REDFLAG
    };
    if target.powerups[flag] != 0 && target_team != attacker_team {
        attacker.last_hurt_carrier = level_time;
    }
}

/// `Team_FragBonuses` for `target` killed by `attacker`, in importance order, one at
/// most: the carrier fragged; one who hurt the killer's carrier fragged; a kill near the
/// killer's flag at its base; a kill near the killer's carrier.
pub fn frag_bonuses(world: &mut dyn FlagWorld, target: u16, attacker: u16, level_time: i32) {
    if target == attacker {
        return;
    }
    let Some((team, targ_origin, carries_red, carries_blue, hurt)) =
        world.player(target).map(|player| {
            (
                player.team,
                player.current_origin,
                player.state.powerups[PW_REDFLAG] != 0,
                player.state.powerups[PW_BLUEFLAG] != 0,
                player.team_state.last_hurt_carrier,
            )
        })
    else {
        return;
    };
    let Some((attacker_team, attacker_origin)) = world
        .player(attacker)
        .map(|player| (player.team, player.current_origin))
    else {
        return;
    };
    // `OnSameTeam`, and nobody's flags outside the two teams.
    if team == attacker_team || !(team == TEAM_RED || team == TEAM_BLUE) {
        return;
    }
    let (flag_pw, carries_enemy_flag) = if team == TEAM_RED {
        (PW_REDFLAG, carries_blue)
    } else {
        (PW_BLUEFLAG, carries_red)
    };
    let other_team = if team == TEAM_RED {
        TEAM_BLUE
    } else {
        TEAM_RED
    };
    if carries_enemy_flag {
        if let Some(player) = world.player(attacker) {
            player.team_state.last_fragged_carrier = level_time;
            player.team_state.frag_carrier += 1;
        }
        world.add_score(attacker, CTF_FRAG_CARRIER_BONUS);
        print_ctf_message(
            world,
            i32::from(attacker),
            team,
            CTFMESSAGE_FRAGGED_FLAG_CARRIER,
        );
        for number in 0..world.slots() {
            if let Some(player) = world.player(number)
                && player.team == other_team
            {
                player.team_state.last_hurt_carrier = 0;
            }
        }
        return;
    }
    // One who hurt the killer's carrier lately (the game tests it twice, with and without
    // the killer carrying a flag, to the same end).
    if hurt != 0 && level_time - hurt < CTF_CARRIER_DANGER_PROTECT_TIMEOUT {
        world.add_score(attacker, CTF_CARRIER_DANGER_PROTECT_BONUS);
        if let Some(player) = world.player(attacker) {
            player.team_state.carrier_defense += 1;
            player.state.persistent[PERS_DEFEND_COUNT] =
                player.state.persistent[PERS_DEFEND_COUNT].wrapping_add(1);
        }
        if let Some(player) = world.player(target) {
            player.team_state.last_hurt_carrier = 0;
        }
        return;
    }
    // The killer's flag at its base, and the killer's carrier.
    if attacker_team != TEAM_RED && attacker_team != TEAM_BLUE {
        return;
    }
    let mut carrier = None;
    for number in 0..world.slots() {
        if let Some(player) = world.player(number)
            && player.state.powerups[flag_pw] != 0
        {
            carrier = Some((number, player.current_origin));
            break;
        }
    }
    let Some(flag) = world.base_flag(attacker_team) else {
        return;
    };
    let v1: [f32; 3] = std::array::from_fn(|axis| targ_origin[axis] - flag[axis]);
    let v2: [f32; 3] = std::array::from_fn(|axis| attacker_origin[axis] - flag[axis]);
    let near_flag = (length(v1) < CTF_TARGET_PROTECT_RADIUS && world.in_pvs(flag, targ_origin))
        || (length(v2) < CTF_TARGET_PROTECT_RADIUS && world.in_pvs(flag, attacker_origin));
    if near_flag && attacker_team != team {
        world.add_score(attacker, CTF_FLAG_DEFENSE_BONUS);
        if let Some(player) = world.player(attacker) {
            player.team_state.base_defense += 1;
            player.state.persistent[PERS_DEFEND_COUNT] =
                player.state.persistent[PERS_DEFEND_COUNT].wrapping_add(1);
        }
        return;
    }
    if let Some((number, carrier_origin)) = carrier
        && number != attacker
    {
        // The game measures the target's distance and then overwrites it with the
        // attacker's; the second test reuses the attacker's distance from the flag.
        let v1: [f32; 3] = std::array::from_fn(|axis| attacker_origin[axis] - carrier_origin[axis]);
        let near_carrier = (length(v1) < CTF_ATTACKER_PROTECT_RADIUS
            && world.in_pvs(carrier_origin, targ_origin))
            || (length(v2) < CTF_ATTACKER_PROTECT_RADIUS
                && world.in_pvs(carrier_origin, attacker_origin));
        if near_carrier && attacker_team != team {
            world.add_score(attacker, CTF_CARRIER_PROTECT_BONUS);
            if let Some(player) = world.player(attacker) {
                player.team_state.carrier_defense += 1;
                player.state.persistent[PERS_DEFEND_COUNT] =
                    player.state.persistent[PERS_DEFEND_COUNT].wrapping_add(1);
            }
        }
    }
}
