//! BaseJKA multiplayer weapon timing and ammo data.
//!
//! The values are the five fields consumed by `PM_Weapon` from codemp's
//! `weaponData[]` table (`game/bg_weapons.c:51-375`). Keeping the complete
//! `weapon_t` table here makes fire prediction and weapon selection share one
//! compatibility-layer source of truth.

/// Number of entries in codemp's multiplayer `weapon_t` enumeration.
pub const LEGACY_WEAPON_COUNT: usize = 19;

/// The subset of `weaponData_t` consumed by gun prediction and selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyWeaponData {
    /// Index into `playerState_t::ammo`.
    pub ammo_index: usize,
    /// Ammo consumed by primary fire.
    pub primary_cost: i32,
    /// Milliseconds added by primary fire.
    pub primary_time: i32,
    /// Ammo consumed by alternate fire.
    pub alternate_cost: i32,
    /// Milliseconds added by alternate fire.
    pub alternate_time: i32,
}

const fn weapon(
    ammo_index: usize,
    primary_cost: i32,
    primary_time: i32,
    alternate_cost: i32,
    alternate_time: i32,
) -> LegacyWeaponData {
    LegacyWeaponData {
        ammo_index,
        primary_cost,
        primary_time,
        alternate_cost,
        alternate_time,
    }
}

/// `weaponData[WP_*]` in `weapon_t` order.
pub const LEGACY_WEAPON_DATA: [LegacyWeaponData; LEGACY_WEAPON_COUNT] = [
    weapon(0, 0, 0, 0, 0),         // WP_NONE
    weapon(0, 0, 400, 0, 400),     // WP_STUN_BATON
    weapon(0, 0, 400, 0, 400),     // WP_MELEE
    weapon(0, 0, 100, 0, 100),     // WP_SABER
    weapon(2, 0, 800, 0, 800),     // WP_BRYAR_PISTOL
    weapon(2, 2, 350, 3, 150),     // WP_BLASTER
    weapon(3, 5, 600, 6, 1_300),   // WP_DISRUPTOR
    weapon(3, 5, 1_000, 5, 750),   // WP_BOWCASTER
    weapon(4, 1, 100, 15, 800),    // WP_REPEATER
    weapon(3, 8, 500, 6, 900),     // WP_DEMP2
    weapon(4, 10, 700, 15, 800),   // WP_FLECHETTE
    weapon(5, 1, 900, 2, 1_200),   // WP_ROCKET_LAUNCHER
    weapon(7, 1, 800, 1, 400),     // WP_THERMAL
    weapon(8, 1, 800, 1, 400),     // WP_TRIP_MINE
    weapon(9, 1, 800, 0, 400),     // WP_DET_PACK
    weapon(4, 40, 800, 50, 1_200), // WP_CONCUSSION
    weapon(2, 2, 400, 2, 400),     // WP_BRYAR_OLD
    weapon(0, 0, 100, 0, 100),     // WP_EMPLACED_GUN
    weapon(0, 0, 0, 0, 0),         // WP_TURRET
];

/// `WPTable` (`bg_saga.c:88-113`): `weapon_t` by the name a definition file writes,
/// `NULL` and `WP_BLASTER_PISTOL` included.
pub const WEAPON_NAMES: [(&str, i32); 21] = [
    ("NULL", 0),
    ("WP_NONE", 0),
    ("WP_STUN_BATON", 1),
    ("WP_MELEE", 2),
    ("WP_SABER", 3),
    ("WP_BRYAR_PISTOL", 4),
    ("WP_BLASTER_PISTOL", 4),
    ("WP_BLASTER", 5),
    ("WP_DISRUPTOR", 6),
    ("WP_BOWCASTER", 7),
    ("WP_REPEATER", 8),
    ("WP_DEMP2", 9),
    ("WP_FLECHETTE", 10),
    ("WP_ROCKET_LAUNCHER", 11),
    ("WP_THERMAL", 12),
    ("WP_TRIP_MINE", 13),
    ("WP_DET_PACK", 14),
    ("WP_CONCUSSION", 15),
    ("WP_BRYAR_OLD", 16),
    ("WP_EMPLACED_GUN", 17),
    ("WP_TURRET", 18),
];

/// `GetIDForString(WPTable, name)`: the weapon a name means, any case.
pub fn weapon_by_name(name: &[u8]) -> Option<i32> {
    WEAPON_NAMES
        .iter()
        .find(|(known, _)| known.as_bytes().eq_ignore_ascii_case(name))
        .map(|(_, weapon)| *weapon)
}

/// Look up one bounded legacy weapon-data entry.
pub fn legacy_weapon_data(weapon: u8) -> Option<LegacyWeaponData> {
    LEGACY_WEAPON_DATA.get(usize::from(weapon)).copied()
}
