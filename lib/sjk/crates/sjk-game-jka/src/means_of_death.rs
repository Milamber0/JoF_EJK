//! `meansOfDeath_t` (`codemp/game/bg_public.h:1105-1156`): how something was hurt or
//! killed, as `G_Damage`, the obituary and the scoreboard name it. One table for the
//! whole game; the numbers are the enum's order.

/// `MOD_UNKNOWN`.
pub const MOD_UNKNOWN: u32 = 0;
/// `MOD_STUN_BATON`.
pub const MOD_STUN_BATON: u32 = 1;
/// `MOD_MELEE`.
pub const MOD_MELEE: u32 = 2;
/// `MOD_SABER`.
pub const MOD_SABER: u32 = 3;
/// `MOD_BRYAR_PISTOL`.
pub const MOD_BRYAR_PISTOL: u32 = 4;
/// `MOD_BRYAR_PISTOL_ALT`.
pub const MOD_BRYAR_PISTOL_ALT: u32 = 5;
/// `MOD_BLASTER`.
pub const MOD_BLASTER: u32 = 6;
/// `MOD_TURBLAST`.
pub const MOD_TURBLAST: u32 = 7;
/// `MOD_DISRUPTOR`.
pub const MOD_DISRUPTOR: u32 = 8;
/// `MOD_DISRUPTOR_SPLASH`.
pub const MOD_DISRUPTOR_SPLASH: u32 = 9;
/// `MOD_DISRUPTOR_SNIPER`.
pub const MOD_DISRUPTOR_SNIPER: u32 = 10;
/// `MOD_BOWCASTER`.
pub const MOD_BOWCASTER: u32 = 11;
/// `MOD_REPEATER`.
pub const MOD_REPEATER: u32 = 12;
/// `MOD_REPEATER_ALT`.
pub const MOD_REPEATER_ALT: u32 = 13;
/// `MOD_REPEATER_ALT_SPLASH`.
pub const MOD_REPEATER_ALT_SPLASH: u32 = 14;
/// `MOD_DEMP2`.
pub const MOD_DEMP2: u32 = 15;
/// `MOD_DEMP2_ALT`.
pub const MOD_DEMP2_ALT: u32 = 16;
/// `MOD_FLECHETTE`.
pub const MOD_FLECHETTE: u32 = 17;
/// `MOD_FLECHETTE_ALT_SPLASH`.
pub const MOD_FLECHETTE_ALT_SPLASH: u32 = 18;
/// `MOD_ROCKET`.
pub const MOD_ROCKET: u32 = 19;
/// `MOD_ROCKET_SPLASH`.
pub const MOD_ROCKET_SPLASH: u32 = 20;
/// `MOD_ROCKET_HOMING`.
pub const MOD_ROCKET_HOMING: u32 = 21;
/// `MOD_ROCKET_HOMING_SPLASH`.
pub const MOD_ROCKET_HOMING_SPLASH: u32 = 22;
/// `MOD_THERMAL`.
pub const MOD_THERMAL: u32 = 23;
/// `MOD_THERMAL_SPLASH`.
pub const MOD_THERMAL_SPLASH: u32 = 24;
/// `MOD_TRIP_MINE_SPLASH`.
pub const MOD_TRIP_MINE_SPLASH: u32 = 25;
/// `MOD_TIMED_MINE_SPLASH`.
pub const MOD_TIMED_MINE_SPLASH: u32 = 26;
/// `MOD_DET_PACK_SPLASH`.
pub const MOD_DET_PACK_SPLASH: u32 = 27;
/// `MOD_VEHICLE`.
pub const MOD_VEHICLE: u32 = 28;
/// `MOD_CONC`.
pub const MOD_CONC: u32 = 29;
/// `MOD_CONC_ALT`.
pub const MOD_CONC_ALT: u32 = 30;
/// `MOD_FORCE_DARK`.
pub const MOD_FORCE_DARK: u32 = 31;
/// `MOD_SENTRY`.
pub const MOD_SENTRY: u32 = 32;
/// `MOD_WATER`.
pub const MOD_WATER: u32 = 33;
/// `MOD_SLIME`.
pub const MOD_SLIME: u32 = 34;
/// `MOD_LAVA`.
pub const MOD_LAVA: u32 = 35;
/// `MOD_CRUSH`.
pub const MOD_CRUSH: u32 = 36;
/// `MOD_TELEFRAG`.
pub const MOD_TELEFRAG: u32 = 37;
/// `MOD_FALLING`.
pub const MOD_FALLING: u32 = 38;
/// `MOD_SUICIDE`.
pub const MOD_SUICIDE: u32 = 39;
/// `MOD_TARGET_LASER`.
pub const MOD_TARGET_LASER: u32 = 40;
/// `MOD_TRIGGER_HURT`.
pub const MOD_TRIGGER_HURT: u32 = 41;
/// `MOD_TEAM_CHANGE`.
pub const MOD_TEAM_CHANGE: u32 = 42;
