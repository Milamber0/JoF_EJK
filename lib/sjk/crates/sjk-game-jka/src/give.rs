//! The `give` cheat: `Cmd_Give_f`/`G_Give` (OpenJK `codemp/game/g_cmds.c:222-355`), a
//! command the game runs only with `sv_cheats` on (`CMD_CHEAT`) and for the living
//! (`CMD_ALIVE`). What it gives: everything at once, health or armour (to the maximum or
//! as asked), Force, every usable weapon or one by number, ammo (999 or as asked) for
//! every kind, or an award; an item by name is spawned on the player and touched, which
//! waits on items. The `weapons` driver hands its shooter the blaster and the bowcaster
//! this way, so the whole-game transcript's dumps hold `give`'s results.

use crate::userinfo::atoi;
use sjk_protocol::PlayerState;

/// `STAT_HOLDABLE_ITEMS`, `STAT_WEAPONS`, `STAT_ARMOR`, `STAT_MAX_HEALTH`.
const STAT_HOLDABLE_ITEMS: usize = 2;
const STAT_WEAPONS: usize = 4;
const STAT_ARMOR: usize = 5;
const STAT_MAX_HEALTH: usize = 8;
/// `HI_NUM_HOLDABLE`.
const HOLDABLES: u32 = 12;
/// `LAST_USEABLE_WEAPON`: `WP_BRYAR_OLD`.
const LAST_USEABLE_WEAPON: u32 = 16;
/// `AMMO_BLASTER..AMMO_MAX`.
const AMMO_KINDS: std::ops::Range<usize> = 2..10;
/// `fd.forcePower`, and `FORCE_POWER_MAX` (`fd.forcePowerMax` is not on the wire; a
/// player's is the constant).
const PS_FORCE_POWER: usize = 18;
const FORCE_POWER_MAX: i32 = 100;
/// The awards, by their `PERS_*_COUNT` slot.
const AWARDS: [(&[u8], usize); 5] = [
    (b"excellent", 10),
    (b"impressive", 9),
    (b"gauntletaward", 13),
    (b"defend", 11),
    (b"assist", 12),
];

/// What `give` came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Given {
    /// The state changed as asked (or the name is `all`).
    Done,
    /// The name is an item's, which is spawned and touched in the reference: not ported.
    Item,
}

/// `G_Give` for `name` with `arguments` (the words after it; `argc == 3` means exactly
/// one), on a living player of `health` with the state's maximum.
pub fn give(state: &mut PlayerState, health: &mut i32, name: &[u8], arguments: &[&[u8]]) -> Given {
    let all = name.eq_ignore_ascii_case(b"all");
    let is = |what: &[u8]| all || name.eq_ignore_ascii_case(what);
    let asked = (arguments.len() == 1).then(|| atoi(arguments[0]));
    let max_health = state.stats[STAT_MAX_HEALTH] as i32;
    if all {
        state.stats[STAT_HOLDABLE_ITEMS] |= (1 << HOLDABLES) - 1;
    }
    if is(b"health") {
        *health = asked.map_or(max_health, |value| value.clamp(1, max_health));
        if !all {
            return Given::Done;
        }
    }
    if is(b"armor") || is(b"shield") {
        state.stats[STAT_ARMOR] =
            asked.map_or(max_health, |value| value.clamp(0, max_health)) as u32;
        if !all {
            return Given::Done;
        }
    }
    if is(b"force") {
        state.set_raw_field(
            PS_FORCE_POWER,
            asked.map_or(FORCE_POWER_MAX, |value| value.clamp(0, FORCE_POWER_MAX)) as u32,
        );
        if !all {
            return Given::Done;
        }
    }
    if is(b"weapons") {
        state.stats[STAT_WEAPONS] = (1 << (LAST_USEABLE_WEAPON + 1)) - 1;
        if !all {
            return Given::Done;
        }
    }
    if !all && name.eq_ignore_ascii_case(b"weaponnum") {
        let number = arguments.first().map_or(0, |word| atoi(word));
        state.stats[STAT_WEAPONS] |= 1_u32.wrapping_shl(number as u32);
        return Given::Done;
    }
    if is(b"ammo") {
        let amount = asked.map_or(999, |value| value.clamp(0, 999)) as u32;
        for kind in AMMO_KINDS {
            state.ammo[kind] = amount;
        }
        if !all {
            return Given::Done;
        }
    }
    for (award, slot) in AWARDS {
        if name.eq_ignore_ascii_case(award) {
            state.persistent[slot] = state.persistent[slot].wrapping_add(1);
            return Given::Done;
        }
    }
    if all { Given::Done } else { Given::Item }
}
