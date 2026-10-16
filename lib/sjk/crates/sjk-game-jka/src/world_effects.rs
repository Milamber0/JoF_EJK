//! `P_WorldEffects` (`codemp/game/g_active.c:146-217`): what the world does to a player
//! at the end of every server frame — drowning once its air is out, and the sizzle of lava
//! and slime.
//!
//! The rules run in two halves because the reference's second half reads what the first
//! did: a drowning's `G_Damage` and its pain debounce come before the sizzle's check of
//! `health` and `pain_debounce_time`. The caller runs [`drown`], applies the damage it asks
//! for, then runs [`sizzle`] with the player as that left it.

use crate::crt_rand::CrtRand;
use crate::damage::DAMAGE_NO_ARMOR;
use crate::means_of_death::{MOD_LAVA, MOD_SLIME, MOD_WATER};

/// `CONTENTS_LAVA`.
pub const CONTENTS_LAVA: u32 = 0x2;
/// `CONTENTS_SLIME`.
pub const CONTENTS_SLIME: u32 = 0x2_0000;
/// `PW_BATTLESUIT`: the powerup that gives air and keeps lava and slime off.
pub const PW_BATTLESUIT: usize = 2;
/// `EV_POWERUP_BATTLESUIT`: what a battlesuit in lava or slime shows instead of damage.
pub const EV_POWERUP_BATTLESUIT: u32 = 95;
/// The two sounds a drowning player makes (`G_Sound(ent, CHAN_VOICE, ...)`).
pub const GURP1: &str = "sound/player/gurp1.wav";
pub const GURP2: &str = "sound/player/gurp2.wav";
/// `CHAN_VOICE`: the channel the gurp is played on.
pub const CHAN_VOICE: u32 = 3;
/// How long a player's air lasts under water (`airOutTime = level.time + 12000`), and how
/// long a battlesuit's (`+ 10000`).
pub const AIR_TIME: i32 = 12_000;
const SUIT_AIR_TIME: i32 = 10_000;

/// What `gclient_t` and `gentity_t` keep for drowning: `airOutTime` and `ent->damage`,
/// which grows by two for every second under water.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Breath {
    /// `client->airOutTime`: when the air runs out. `ClientSpawn` sets it twelve seconds on.
    pub air_out_time: i32,
    /// `ent->damage`: the next drowning blow, 2 to 15. Zero until the player has first
    /// been out of water (a new entity's), 2 from then on.
    pub drown_damage: i32,
}

impl Breath {
    /// `ClientSpawn`'s `client->airOutTime = level.time + 12000` (`g_client.c:3335`);
    /// `ent->damage` is the entity's and outlives the spawn.
    pub fn spawned(&mut self, level_time: i32) {
        self.air_out_time = level_time + AIR_TIME;
    }
}

/// The player as `P_WorldEffects` reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surroundings {
    /// `level.time`.
    pub level_time: i32,
    /// `ent->waterlevel` and `ent->watertype`, from the player's last move.
    pub water_level: u8,
    pub water_type: u32,
    /// `ent->client->noclip`.
    pub noclip: bool,
    /// `ent->health`.
    pub health: i32,
    /// `ps.powerups[PW_BATTLESUIT]`.
    pub battlesuit_until: i32,
    /// `client->tempSpectate`: a siege player waiting as a spectator is spared.
    pub temp_spectate: i32,
    /// `ent->pain_debounce_time`.
    pub pain_debounce_time: i32,
}

/// A drowning blow: its gurp, the pain debounce it sets, and the damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drowning {
    /// [`GURP1`] or [`GURP2`], played on the player's voice channel.
    pub sound: &'static str,
    /// `pain_debounce_time = level.time + 200`: no pain sound of its own.
    pub pain_debounce_time: i32,
    /// `G_Damage(ent, NULL, NULL, NULL, NULL, damage, DAMAGE_NO_ARMOR, MOD_WATER)`.
    pub damage: i32,
    pub flags: u32,
    pub means: u32,
}

/// The first half of `P_WorldEffects`: the air counted, and a blow when it has run out.
/// `rand` is the C library's (`rand() & 1` picks the gurp), drawn only for a blow that
/// does not kill.
pub fn drown(breath: &mut Breath, player: &Surroundings, rand: &mut CrtRand) -> Option<Drowning> {
    let level_time = player.level_time;
    if player.noclip {
        breath.air_out_time = level_time + AIR_TIME;
        return None;
    }
    if player.water_level != 3 {
        breath.air_out_time = level_time + AIR_TIME;
        breath.drown_damage = 2;
        return None;
    }
    if player.battlesuit_until > level_time {
        breath.air_out_time = level_time + SUIT_AIR_TIME;
    }
    if breath.air_out_time >= level_time {
        return None;
    }
    breath.air_out_time += 1000;
    if player.health <= 0 || player.temp_spectate >= level_time {
        return None;
    }
    breath.drown_damage = (breath.drown_damage + 2).min(15);
    let sound = if player.health <= breath.drown_damage || rand.next() & 1 != 0 {
        GURP1
    } else {
        GURP2
    };
    Some(Drowning {
        sound,
        pain_debounce_time: level_time + 200,
        damage: breath.drown_damage,
        flags: DAMAGE_NO_ARMOR,
        means: MOD_WATER,
    })
}

/// What lava or slime does to a player in it this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sizzle {
    /// A battlesuit's `G_AddEvent(ent, EV_POWERUP_BATTLESUIT, 0)` instead of the burns.
    pub battlesuit: bool,
    /// `G_Damage(ent, NULL, NULL, NULL, NULL, 30 * waterlevel, 0, MOD_LAVA)`.
    pub lava: Option<i32>,
    /// `G_Damage(ent, NULL, NULL, NULL, NULL, 10 * waterlevel, 0, MOD_SLIME)`, after the lava's.
    pub slime: Option<i32>,
}

/// The second half of `P_WorldEffects`, with the player as the first half's damage left it.
/// The slime's blow reads the player after the lava's, which the caller applies between the
/// two (both are in one liquid only on a map that mixes them in one brush).
pub fn sizzle(player: &Surroundings) -> Sizzle {
    let level_time = player.level_time;
    let liquid = player.water_type & (CONTENTS_LAVA | CONTENTS_SLIME);
    if player.noclip || player.water_level == 0 || liquid == 0 {
        return Sizzle::default();
    }
    if player.health <= 0
        || player.temp_spectate >= level_time
        || player.pain_debounce_time > level_time
    {
        return Sizzle::default();
    }
    if player.battlesuit_until > level_time {
        return Sizzle {
            battlesuit: true,
            ..Sizzle::default()
        };
    }
    let level = i32::from(player.water_level);
    Sizzle {
        battlesuit: false,
        lava: (liquid & CONTENTS_LAVA != 0).then_some(30 * level),
        slime: (liquid & CONTENTS_SLIME != 0).then_some(10 * level),
    }
}

/// `MOD_LAVA` and `MOD_SLIME`, for the caller's damage requests.
pub const LAVA: u32 = MOD_LAVA;
pub const SLIME: u32 = MOD_SLIME;
