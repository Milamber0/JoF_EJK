//! Siege's repairs (`TryHeal`, OpenJK `codemp/game/g_utils.c:1522-1578`): a thing the
//! map marks with a `healingclass` — an emplaced gun, a turret, a vehicle — is mended ten
//! points at a time, every `healingrate` milliseconds, by a player of that siege class
//! holding the use key on it, who is kept in the use pose meanwhile. The map's
//! `healingsound` is precached as the thing spawns (`g_spawn.c:722-723`) and played on it
//! at every repair.

use sjk_entity::Entity;

/// `BOTH_BUTTON_HOLD`, `BOTH_CONSOLE1`: the use poses a repair keeps its player in.
pub const BOTH_BUTTON_HOLD: u16 = 1_328;
pub const BOTH_CONSOLE1: u16 = 954;
/// `GT_SIEGE`.
const GT_SIEGE: i32 = 7;

/// What a map gave a thing to be repaired by.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Healable {
    /// `healingclass`: the siege class that repairs it; `None` for a thing nobody does.
    pub class: Option<Vec<u8>>,
    /// `healingrate`: milliseconds between two repairs.
    pub rate: i32,
    /// `healingsound`, played on the thing at each repair.
    pub sound: Option<Vec<u8>>,
    /// `healingDebounce`: no repair before this time.
    pub debounce: i32,
}

impl Healable {
    /// The three spawn keys (`F_STRING`, `F_INT`, `F_STRING`), the last of each the lump
    /// gives.
    pub fn from_entity(entity: &Entity) -> Self {
        let last = |wanted: &str| {
            entity
                .fields()
                .iter()
                .rev()
                .find(|(key, _)| key.eq_ignore_ascii_case(wanted))
                .map(|(_, value)| value.as_bytes().to_vec())
        };
        Self {
            class: last("healingclass"),
            rate: last("healingrate").map_or(0, |value| crate::userinfo::atoi(&value)),
            sound: last("healingsound").filter(|sound| !sound.is_empty()),
            debounce: 0,
        }
    }
}

/// What a repair attempt came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heal {
    /// Not this player's to repair (not siege, no class, not its class, nothing to mend).
    No,
    /// Its class's: the player held in the use pose; `repaired` when ten points were
    /// mended this time (the health bar and the sound are the caller's).
    Yes { repaired: bool },
}

/// `TryHeal` for a thing of `health` out of `max_health`, by a player of siege class
/// `healer` (`None` for none), in game type `gametype`, at `level_time`.
pub fn try_heal(
    healable: &mut Healable,
    health: &mut i32,
    max_health: i32,
    healer: Option<&[u8]>,
    gametype: i32,
    level_time: i32,
) -> Heal {
    let (Some(class), Some(healer)) = (healable.class.as_deref(), healer) else {
        return Heal::No;
    };
    if gametype != GT_SIEGE
        || max_health == 0
        || class.is_empty()
        || *health <= 0
        || *health >= max_health
        || !class.eq_ignore_ascii_case(healer)
    {
        return Heal::No;
    }
    let repaired = healable.debounce < level_time;
    if repaired {
        *health = (*health + 10).min(max_health);
        healable.debounce = level_time + healable.rate;
    }
    Heal::Yes { repaired }
}

/// The pose a repair keeps its player in (`TryHeal`'s end): an extended use pose
/// (`torsoTimer` 500) when already in one, else the use pose begun.
pub fn healer_pose(torso_anim: u16) -> Option<u16> {
    (torso_anim != BOTH_BUTTON_HOLD && torso_anim != BOTH_CONSOLE1).then_some(BOTH_BUTTON_HOLD)
}
