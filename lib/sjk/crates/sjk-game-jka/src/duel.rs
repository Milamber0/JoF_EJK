//! Private duels (`Cmd_EngageDuel_f`, `g_cmds.c:2996-3143`, and `ClientThink_real`'s duel,
//! `g_active.c:2405-2528`): outside the team games and the duel game types, a player with
//! its saber in hand challenges the one it aims at within 256 units; the other, aiming
//! back while the challenge stands (five seconds), accepts. Both are holstered and held
//! still two seconds, then their sabers light and they fight each other alone — no one
//! else can hurt them or be hurt by them (`G_Damage`, `g_combat.c:4540-4556`; a missile at
//! a duellist is spent on it harmlessly, `G_MissileImpact`'s `killProj`), and between them
//! only the saber hurts. The duel ends when one dies (the other healed to its maximum and
//! protected as at a spawn, everyone told), when either leaves, or when they are 1024
//! units apart.
//!
//! The wire fields are the player state's own (`duelInProgress`, `duelIndex`,
//! `duelTime`); the movement freezes and arms a duellist from them.

use sjk_protocol::PlayerState;

/// `EV_PRIVATE_DUEL`: 1 accepted, 2 begun, 0 over.
pub const EV_PRIVATE_DUEL: u32 = 15;
const PS_WEAPON_TIME: usize = 10;
const PS_EFLAGS: usize = 17;
const PS_SPEED: usize = 12;
const PS_DUEL_INDEX: usize = 44;
const PS_WEAPON: usize = 47;
const PS_FORCE_HAND_EXTEND: usize = 80;
const PS_SABER_HOLSTERED: usize = 81;
const PS_SABER_IN_FLIGHT: usize = 88;
const PS_DUEL_TIME: usize = 118;
const PS_DUEL_IN_PROGRESS: usize = 119;
const STAT_HEALTH: usize = 0;
const STAT_MAX_HEALTH: usize = 8;
const WP_SABER: u32 = 3;
const HANDEXTEND_DUELCHALLENGE: u32 = 9;
const EF_INVULNERABLE: u32 = 1 << 27;
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;
const GT_TEAM: i32 = 6;
const MOD_SABER: u32 = 3;
const MAX_CLIENTS: u16 = 32;
/// `g_spawnInvulnerability`: the winner's protection.
const SPAWN_INVULNERABILITY: i32 = 3_000;

/// Whether `state`'s player duels somebody other than player `other`.
pub fn elsewhere(state: &PlayerState, other: u16) -> bool {
    state.duel_in_progress() && state.duel_index() != other
}

/// `G_Damage`'s duel rule (`g_combat.c:4540-4556`): a duellist is hurt by nobody but its
/// opponent, and by it only with the saber; a duellist hurts nobody but its opponent, and
/// it only with the saber. `attacker` is a client's number and state.
pub fn damage_refused(
    target: u16,
    target_state: &PlayerState,
    attacker: Option<(u16, &PlayerState)>,
    means: u32,
) -> bool {
    let Some((attacker, attacker_state)) = attacker else {
        return false;
    };
    if target_state.duel_in_progress()
        && (attacker != target_state.duel_index() || means != MOD_SABER)
    {
        return true;
    }
    attacker_state.duel_in_progress()
        && (target != attacker_state.duel_index() || means != MOD_SABER)
}

/// A player as a duel reads and changes it.
pub struct Duellist<'a> {
    pub number: u16,
    pub state: &'a mut PlayerState,
    /// `gentity_t::health`.
    pub health: &'a mut i32,
    /// `pers.netname`, as messages print it.
    pub name: &'a [u8],
    /// `forceHandExtendTime` (the knockdown's memory).
    pub hand_extend_time: &'a mut i32,
    /// `invulnerableTimer`.
    pub invulnerable_until: &'a mut i32,
    /// `sess.sessionTeam`, `inuse`.
    pub team: i32,
}

/// What a challenge or a duel's think asks the caller to do besides the states.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Told {
    /// Server commands: to a client, or to everyone (`None`).
    pub commands: Vec<(Option<u16>, Vec<u8>)>,
    /// Saber sounds (`G_Sound` on `CHAN_AUTO`) on a player: `true` its on-sound, `false`
    /// its off-sound — for each of its two sabers, the second counted as the game counts
    /// it.
    pub sounds: Vec<(u16, crate::generic_commands::SaberSound)>,
    /// The players an event was added to (`G_AddEvent`: `eventTime` is the caller's).
    pub raised: Vec<u16>,
}

/// `G_AddEvent(player, EV_PRIVATE_DUEL, parameter)`.
fn add_event(state: &mut PlayerState, parameter: u32) {
    crate::player_entity::add_event(state, EV_PRIVATE_DUEL, parameter);
}

/// The line `Cmd_EngageDuel_f` traces (`MASK_PLAYERSOLID`, from the origin): 256 units
/// along the view, to eye height.
pub fn challenge_line(state: &PlayerState) -> ([f32; 3], [f32; 3]) {
    let origin = state.origin();
    let forward = crate::pmove::flight::flight_axes(state.view_angles())
        .0
        .to_array();
    let view_height = state.raw_field(22).unwrap_or(0) as i32 as f32;
    let end = [
        origin[0] + forward[0] * 256.0,
        origin[1] + forward[1] * 256.0,
        (origin[2] + view_height) + forward[2] * 256.0,
    ];
    (origin, end)
}

/// Whether `Cmd_EngageDuel_f` goes as far as its trace for this player: private duels on,
/// a free-for-all, not holding off a challenge of its own, the saber in hand and not
/// thrown, no duel already.
pub fn may_challenge(
    state: &PlayerState,
    gametype: i32,
    private_duels: bool,
    level_time: i32,
) -> bool {
    let field = |index: usize| state.raw_field(index).unwrap_or(0);
    private_duels
        && gametype != GT_DUEL
        && gametype != GT_POWERDUEL
        && gametype < GT_TEAM
        && state.duel_time() < level_time
        && field(PS_WEAPON) == WP_SABER
        && field(PS_SABER_IN_FLIGHT) == 0
        && !state.duel_in_progress()
}

/// The rest of `Cmd_EngageDuel_f` for `me`, whose trace struck `other` (a client
/// number): accepted if `other` challenged `me` and its challenge stands, else a
/// challenge — either way `me` gestures and names `other` for five seconds. Nothing for
/// one dead, without its saber in hand, duelling or with its saber thrown.
pub fn engage(me: &mut Duellist, other: &mut Duellist, gametype: i32, level_time: i32) -> Told {
    let mut told = Told::default();
    let field = |state: &PlayerState, index: usize| state.raw_field(index).unwrap_or(0);
    if other.number >= MAX_CLIENTS
        || *other.health < 1
        || (other.state.stats[STAT_HEALTH] as i32) < 1
        || field(other.state, PS_WEAPON) != WP_SABER
        || other.state.duel_in_progress()
        || field(other.state, PS_SABER_IN_FLIGHT) != 0
    {
        return told;
    }
    if gametype >= GT_TEAM && other.team == me.team {
        return told;
    }
    if other.state.duel_index() == me.number && other.state.duel_time() >= level_time {
        let text = [
            b"print \"".as_slice(),
            other.name,
            b" @@@PLDUELACCEPT ",
            me.name,
            b"!\n\"",
        ]
        .concat();
        told.commands.push((None, text));
        for state in [&mut *me.state, &mut *other.state] {
            state.set_raw_field(PS_DUEL_IN_PROGRESS, 1);
            state.set_raw_field(PS_DUEL_TIME, (level_time + 2_000) as u32);
        }
        for (number, state) in [
            (me.number, &mut *me.state),
            (other.number, &mut *other.state),
        ] {
            add_event(state, 1);
            told.raised.push(number);
        }
        // Holstered until the duel begins, to light them then.
        for (number, state) in [
            (me.number, &mut *me.state),
            (other.number, &mut *other.state),
        ] {
            if field(state, PS_SABER_HOLSTERED) == 0 {
                // `saber[0].soundOff`; the second saber's only with a model.
                told.sounds.extend(
                    crate::generic_commands::SaberSound::both(false).map(|sound| (number, sound)),
                );
                state.set_raw_field(PS_WEAPON_TIME, 400);
                state.set_raw_field(PS_SABER_HOLSTERED, 2);
            }
        }
    } else {
        told.commands.push((
            Some(other.number),
            [b"cp \"".as_slice(), me.name, b" @@@PLDUELCHALLENGE\n\""].concat(),
        ));
        told.commands.push((
            Some(me.number),
            [b"cp \"@@@PLDUELCHALLENGED ".as_slice(), other.name, b"\n\""].concat(),
        ));
    }
    me.state
        .set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_DUELCHALLENGE);
    *me.hand_extend_time = level_time + 1_000;
    me.state
        .set_raw_field(PS_DUEL_INDEX, u32::from(other.number));
    me.state
        .set_raw_field(PS_DUEL_TIME, (level_time + 5_000) as u32);
    told
}

/// Both duellists out of their duel, and told so (`EV_PRIVATE_DUEL` 0).
fn end_both(me: &mut Duellist, other: &mut Duellist, told: &mut Told) {
    for state in [&mut *me.state, &mut *other.state] {
        state.set_raw_field(PS_DUEL_IN_PROGRESS, 0);
    }
    for (number, state) in [
        (me.number, &mut *me.state),
        (other.number, &mut *other.state),
    ] {
        add_event(state, 0);
        told.raised.push(number);
    }
}

/// What the duel's think leaves of the command: held still while the duel has not begun.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Thought {
    /// `speed`, `basespeed` and the command's moves zeroed.
    pub frozen: bool,
}

/// `ClientThink_real`'s duel for `me`, duelling `against` (`None` for one gone): the
/// sabers lit once the wait is over; the duel ended when the other is gone or no longer
/// duels `me`, when it is dead (the winner healed and protected), or when they are 1024
/// units apart.
pub fn think(
    me: &mut Duellist,
    against: Option<&mut Duellist>,
    level_time: i32,
    told: &mut Told,
) -> Thought {
    let mut thought = Thought::default();
    if !me.state.duel_in_progress() {
        return thought;
    }
    let field = |state: &PlayerState, index: usize| state.raw_field(index).unwrap_or(0);
    let mut against = against;
    if me.state.duel_time() < level_time {
        // Bring out the sabers: this player's, then the opponent's.
        let bring_out = |number: u16, state: &mut PlayerState, told: &mut Told| {
            if field(state, PS_WEAPON) == WP_SABER
                && field(state, PS_SABER_HOLSTERED) != 0
                && state.duel_time() != 0
            {
                state.set_raw_field(PS_SABER_HOLSTERED, 0);
                // Both sabers' on-sounds, the empty second's default one too.
                told.sounds.extend(
                    crate::generic_commands::SaberSound::both(true).map(|sound| (number, sound)),
                );
                add_event(state, 2);
                told.raised.push(number);
                state.set_raw_field(PS_DUEL_TIME, 0);
            }
        };
        bring_out(me.number, me.state, told);
        if let Some(other) = against.as_deref_mut() {
            bring_out(other.number, other.state, told);
        }
    } else {
        me.state.set_raw_field(PS_SPEED, 0f32.to_bits());
        thought.frozen = true;
    }
    let Some(other) = against.filter(|other| other.state.duel_index() == me.number) else {
        me.state.set_raw_field(PS_DUEL_IN_PROGRESS, 0);
        add_event(me.state, 0);
        told.raised.push(me.number);
        return thought;
    };
    if *other.health < 1 || (other.state.stats[STAT_HEALTH] as i32) < 1 {
        end_both(me, other, told);
        let alive = *me.health > 0 && me.state.stats[STAT_HEALTH] as i32 > 0;
        if alive {
            let max = me.state.stats[STAT_MAX_HEALTH] as i32;
            if *me.health < max {
                *me.health = max;
                me.state.stats[STAT_HEALTH] = max as u32;
            }
            let flags = field(me.state, PS_EFLAGS);
            me.state.set_raw_field(PS_EFLAGS, flags | EF_INVULNERABLE);
            *me.invulnerable_until = level_time + SPAWN_INVULNERABILITY;
            told.commands.push((
                None,
                [
                    b"cp \"".as_slice(),
                    me.name,
                    b" @@@PLDUELWINNER ",
                    other.name,
                    b"!\n\"",
                ]
                .concat(),
            ));
        } else {
            // Both died in the same frame.
            told.commands
                .push((None, b"cp \"@@@PLDUELTIE\n\"".to_vec()));
        }
        return thought;
    }
    let (mine, theirs) = (me.state.origin(), other.state.origin());
    let apart = (0..3)
        .map(|axis| (mine[axis] - theirs[axis]) * (mine[axis] - theirs[axis]))
        .sum::<f32>()
        .sqrt();
    if apart >= 1_024.0 {
        end_both(me, other, told);
        told.commands
            .push((None, b"print \"@@@PLDUELSTOP\n\"".to_vec()));
    }
    thought
}
