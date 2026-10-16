//! Holdable items: which one is used and whether it may be (OpenJK `codemp/game`
//! `bg_misc.c` `BG_IsItemSelectable`, `BG_CycleInven`; `bg_pmove.c` `PM_ItemUsable`;
//! `g_cmds.c` `G_ItemUsable`), and what using one does to its user (`g_items.c`
//! `ItemUse_MedPack`, `_MedPack_Big`, `_Binoculars`, `_Seeker`, `_Jetpack`, `_UseCloak`;
//! `NPC_AI_Jedi.c` `Jedi_Cloak`/`Jedi_Decloak`; `g_main.c`'s jetpack and cloak batteries).
//!
//! The use key's branch of `PM_Weapon` is [`crate::pmove_holdable`]; the seeker drone's
//! think is [`crate::seeker_drone`]. The shield, the sentry and the E-Web place entities
//! of their own; those deployable entities are not implemented yet.

use crate::items::{ITEMS, Kind};
use crate::pmove::{MovementCollision, MovementTrace};

/// `holdable_t`.
pub const HI_NONE: i32 = 0;
pub const HI_SEEKER: i32 = 1;
pub const HI_SHIELD: i32 = 2;
pub const HI_MEDPAC: i32 = 3;
pub const HI_MEDPAC_BIG: i32 = 4;
pub const HI_BINOCULARS: i32 = 5;
pub const HI_SENTRY_GUN: i32 = 6;
pub const HI_JETPACK: i32 = 7;
pub const HI_HEALTHDISP: i32 = 8;
pub const HI_AMMODISP: i32 = 9;
pub const HI_EWEB: i32 = 10;
pub const HI_CLOAK: i32 = 11;
pub const HI_NUM_HOLDABLE: i32 = 12;
/// `EV_USE_ITEM0`: `EV_USE_ITEM0 + tag` uses the item of that tag.
pub const EV_USE_ITEM0: u16 = 45;
/// `EV_ITEMUSEFAIL`, with an [`ItemUseFail`] as its parameter.
pub const EV_ITEMUSEFAIL: u16 = 61;
/// `PMF_USE_ITEM_HELD`: the use key has not been let go since it last did something.
pub const PMF_USE_ITEM_HELD: u16 = 1_024;
/// `EF_SEEKERDRONE`, `EF_DEAD`.
pub const EF_SEEKERDRONE: u32 = 0x0020_0000;
pub const EF_DEAD: u32 = 0x0000_0002;
/// `MAX_MEDPACK_HEAL_AMOUNT`, `MAX_MEDPACK_BIG_HEAL_AMOUNT`.
pub const MEDPACK_HEAL: i32 = 25;
pub const MEDPACK_BIG_HEAL: i32 = 50;
/// `JETPACK_TOGGLE_TIME`, `CLOAK_TOGGLE_TIME`.
pub const TOGGLE_TIME: i32 = 1_000;
/// `JETPACK_DEFUEL_RATE`/`CLOAK_DEFUEL_RATE`, `JETPACK_REFUEL_RATE`/`CLOAK_REFUEL_RATE`.
pub const DEFUEL_RATE: i32 = 200;
pub const REFUEL_RATE: i32 = 150;
/// `Q3_INFINITE`: a powerup that lasts until taken off.
pub const Q3_INFINITE: i32 = 16_777_216;
/// `MASK_PLAYERSOLID`, `MASK_SHOT`, `MASK_SOLID`.
const MASK_PLAYERSOLID: u32 = 0x1111;
const MASK_SHOT: u32 = 0x1301;
const MASK_SOLID: u32 = 0x1001;

/// `itemUseFail_t`: why an item was refused, which the user is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemUseFail {
    SentryNoRoom = 1,
    SentryAlreadyPlaced = 2,
    ShieldNoRoom = 3,
    SeekerAlreadyDeployed = 4,
}

/// `bg_itemlist[index].giTag` of a holdable (0 for any other row).
pub fn tag_of(index: u32) -> i32 {
    ITEMS
        .get(index as usize)
        .filter(|item| item.kind == Kind::Holdable)
        .map_or(0, |item| item.tag)
}

/// `BG_GetItemIndexByTag(tag, IT_HOLDABLE)`: the item list's row of the holdable `tag`
/// (0 for none).
pub fn index_of(tag: i32) -> u32 {
    ITEMS
        .iter()
        .position(|item| item.kind == Kind::Holdable && item.tag == tag)
        .map_or(0, |index| index as u32)
}

/// `BG_IsItemSelectable`: the dispensers and the jetpack are never the selected item.
pub fn selectable(tag: i32) -> bool {
    !matches!(tag, HI_HEALTHDISP | HI_AMMODISP | HI_JETPACK)
}

/// Whether using the item leaves it with its user (`PM_Weapon`'s list): binoculars,
/// jetpack, dispensers, cloak, E-Web.
pub fn kept_after_use(tag: i32) -> bool {
    matches!(
        tag,
        HI_BINOCULARS | HI_JETPACK | HI_HEALTHDISP | HI_AMMODISP | HI_CLOAK | HI_EWEB
    )
}

/// `BG_CycleInven(ps, direction)`: the next selectable item held after the selected one
/// (1) or before it (-1), going round once; the selection unchanged when none is.
pub fn cycle(items: u32, selected: u32, direction: i32) -> u32 {
    let original = tag_of(selected);
    let mut tag = original;
    let step = |tag: i32| -> i32 {
        let next = tag + direction;
        if next <= 0 {
            HI_NUM_HOLDABLE - 1
        } else if next >= HI_NUM_HOLDABLE {
            1
        } else {
            next
        }
    };
    tag = step(tag);
    // "if hit nothing then select nothing": the reference keeps the selection.
    for _ in 0..32 {
        if tag == original {
            break;
        }
        if items & (1 << tag) != 0 && selectable(tag) {
            return index_of(tag);
        }
        tag = step(tag);
    }
    selected
}

/// What `PM_ItemUsable` and `G_ItemUsable` read of the user.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Holder {
    pub client: u16,
    pub origin: [f32; 3],
    pub view_angles: [f32; 3],
    /// `stats[STAT_HEALTH]`, `stats[STAT_MAX_HEALTH]`.
    pub health: i32,
    pub max_health: i32,
    pub entity_flags: u32,
    pub movement_flags: u16,
    /// `m_iVehicleNum != 0`.
    pub riding: bool,
    pub duel_in_progress: bool,
    /// `fd.sentryDeployed`.
    pub sentry_deployed: bool,
}

/// `PM_ItemUsable(ps, tag)` (`in_pmove`) or `G_ItemUsable(ps, tag)`: `Ok` when the item
/// may be used; `Err(Some(fail))` when it is refused with an event, `Err(None)` quietly.
/// `trace` is the movement's or the server's, passing through the user.
pub fn usable(
    holder: &Holder,
    tag: i32,
    in_pmove: bool,
    world: &dyn MovementCollision,
) -> Result<(), Option<ItemUseFail>> {
    // `G_ItemUsable`'s own first gate: "dead players shouldn't use items".
    if !in_pmove && holder.health <= 0 {
        return Err(None);
    }
    if holder.riding || holder.movement_flags & PMF_USE_ITEM_HELD != 0 {
        return Err(None);
    }
    // Only the movement's refuses a private duel.
    if in_pmove && holder.duel_in_progress {
        return Err(None);
    }
    if !selectable(tag) {
        return Err(None);
    }
    let trace = |start: [f32; 3],
                 mins: [f32; 3],
                 maxs: [f32; 3],
                 end: [f32; 3],
                 mask: u32|
     -> MovementTrace { world.trace(start, mins, maxs, end, mask) };
    match tag {
        HI_MEDPAC | HI_MEDPAC_BIG => {
            let dead = holder.health <= 0 || (in_pmove && holder.entity_flags & EF_DEAD != 0);
            if holder.health >= holder.max_health || dead {
                Err(None)
            } else {
                Ok(())
            }
        }
        HI_SEEKER if holder.entity_flags & EF_SEEKERDRONE != 0 => {
            Err(Some(ItemUseFail::SeekerAlreadyDeployed))
        }
        HI_SENTRY_GUN => {
            if holder.sentry_deployed {
                return Err(Some(ItemUseFail::SentryAlreadyPlaced));
            }
            let forward = crate::pmove::flight::flight_axes([0.0, holder.view_angles[1], 0.0])
                .0
                .to_array();
            let ahead: [f32; 3] =
                std::array::from_fn(|axis| holder.origin[axis] + forward[axis] * 64.0);
            let test: [f32; 3] = std::array::from_fn(|axis| ahead[axis] + forward[axis] * 16.0);
            let hit = trace(
                holder.origin,
                [-8.0, -8.0, 0.0],
                [8.0, 8.0, 24.0],
                test,
                MASK_PLAYERSOLID,
            );
            if (hit.fraction != 1.0 && hit.entity_number != holder.client)
                || hit.start_solid
                || hit.all_solid
            {
                Err(Some(ItemUseFail::SentryNoRoom))
            } else {
                Ok(())
            }
        }
        HI_SHIELD => {
            let (mins, maxs) = ([-8.0, -8.0, 0.0], [8.0, 8.0, 8.0]);
            let mut forward = crate::pmove::flight::flight_axes(holder.view_angles)
                .0
                .to_array();
            forward[2] = 0.0;
            let dest: [f32; 3] =
                std::array::from_fn(|axis| holder.origin[axis] + 64.0 * forward[axis]);
            let hit = trace(holder.origin, mins, maxs, dest, MASK_SHOT);
            if hit.fraction > 0.9 && !hit.start_solid && !hit.all_solid {
                let pos = hit.end_position;
                let down = trace(
                    pos,
                    mins,
                    maxs,
                    [pos[0], pos[1], pos[2] - 4096.0],
                    MASK_SOLID,
                );
                if !down.start_solid && !down.all_solid {
                    return Ok(());
                }
            }
            Err(Some(ItemUseFail::ShieldNoRoom))
        }
        _ => Ok(()),
    }
}

/// `MedPackGive(ent, amount)`: health up to the maximum, for a living user below it.
pub fn medpack_give(health: i32, max_health: i32, dead: bool, amount: i32) -> i32 {
    if health <= 0 || dead || health >= max_health {
        return health;
    }
    (health + amount).min(max_health)
}

/// `ItemUse_Binoculars`: with the weapon ready, the binoculars' zoom on (`zoomMode` 2,
/// unlocked, 40 degrees) or off (the time noted). Returns the new `zoomMode`,
/// `zoomLocked`, `zoomFov` and `zoomTime` as `(mode, Some((locked, fov)), time)`.
pub fn binoculars(zoom_mode: u8, weapon_ready: bool, level_time: i32) -> Option<Zoom> {
    if !weapon_ready {
        return None;
    }
    match zoom_mode {
        0 => Some(Zoom::On),
        2 => Some(Zoom::Off(level_time)),
        _ => None,
    }
}

/// What [`binoculars`] does to the zoom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    /// `zoomMode` 2, `zoomLocked` off, `zoomFov` 40.
    On,
    /// `zoomMode` 0 and `zoomTime` this.
    Off(i32),
}

/// A user's jetpack and cloak, and their batteries: `jetPackOn`, `jetPackToggleTime`,
/// `jetPackDebReduce`, `jetPackDebRecharge`, `cloakToggleTime`, `cloakDebReduce`,
/// `cloakDebRecharge` of the client (the fuel itself is on the player state).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Packs {
    pub jetpack_on: bool,
    pub jetpack_toggle_time: i32,
    pub jetpack_reduce_at: i32,
    pub jetpack_recharge_at: i32,
    pub cloak_toggle_time: i32,
    pub cloak_reduce_at: i32,
    pub cloak_recharge_at: i32,
}

/// What a player's holdables keep on the server beyond its player state: the packs, and
/// the seeker drone's two clocks (`droneExistTime`, `droneFireTime`, which are not sent).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gear {
    pub packs: Packs,
    pub drone_exist: f32,
    pub drone_fire: f32,
}

/// What a toggle did, for the server to sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggled {
    Nothing,
    /// `sound/boba/JETON` on `CHAN_AUTO`.
    JetpackOn,
    JetpackOff,
    /// `sound/chars/shadowtrooper/cloak.wav` on `CHAN_ITEM`, `FL_NOTARGET` set.
    Cloaked,
    /// `sound/chars/shadowtrooper/decloak.wav`, `FL_NOTARGET` cleared.
    Decloaked,
}

/// The user as the jetpack and cloak read it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wearer {
    pub health: i32,
    pub dead: bool,
    pub jetpack_fuel: i32,
    pub cloak_fuel: i32,
    /// `powerups[PW_CLOAKED]`.
    pub cloaked: bool,
    /// `fd.forceGripBeingGripped >= level.time`.
    pub gripped: bool,
    pub falling_to_death: bool,
}

/// `ItemUse_Jetpack`: on or off, a second apart; never while dead, never switched on with
/// less than 5 fuel, nor while gripped or falling to death (`Jetpack_On`).
pub fn use_jetpack(packs: &mut Packs, wearer: &Wearer, level_time: i32) -> Toggled {
    if packs.jetpack_toggle_time >= level_time || wearer.health <= 0 || wearer.dead {
        return Toggled::Nothing;
    }
    if !packs.jetpack_on && wearer.jetpack_fuel < 5 {
        return Toggled::Nothing;
    }
    let toggled = if packs.jetpack_on {
        packs.jetpack_on = false;
        Toggled::JetpackOff
    } else if wearer.gripped || wearer.falling_to_death {
        Toggled::Nothing
    } else {
        packs.jetpack_on = true;
        Toggled::JetpackOn
    };
    packs.jetpack_toggle_time = level_time + TOGGLE_TIME;
    toggled
}

/// `ItemUse_UseCloak`: cloaked or uncloaked, a second apart; never while dead, never
/// cloaked with less than 5 in the battery.
pub fn use_cloak(packs: &mut Packs, wearer: &Wearer, level_time: i32) -> Toggled {
    if packs.cloak_toggle_time >= level_time || wearer.health <= 0 || wearer.dead {
        return Toggled::Nothing;
    }
    if !wearer.cloaked && wearer.cloak_fuel < 5 {
        return Toggled::Nothing;
    }
    packs.cloak_toggle_time = level_time + TOGGLE_TIME;
    if wearer.cloaked {
        Toggled::Decloaked
    } else {
        Toggled::Cloaked
    }
}

/// `G_RunFrame`'s batteries for one client (`g_main.c:3246-3301`): a jetpack in use drains
/// a point every 200 ms (two while thrusting) and switches off empty; one off recharges a
/// point every 150 ms; the cloak likewise, decloaking empty. Returns what switched off.
pub fn batteries(
    packs: &mut Packs,
    jetpack_fuel: &mut i32,
    cloak_fuel: &mut i32,
    cloaked: bool,
    thrusting: bool,
    level_time: i32,
) -> Toggled {
    let mut off = Toggled::Nothing;
    if packs.jetpack_on {
        if packs.jetpack_reduce_at < level_time {
            *jetpack_fuel -= if thrusting { 2 } else { 1 };
            if *jetpack_fuel <= 0 {
                *jetpack_fuel = 0;
                packs.jetpack_on = false;
                off = Toggled::JetpackOff;
            }
            packs.jetpack_reduce_at = level_time + DEFUEL_RATE;
        }
    } else if *jetpack_fuel < 100 && packs.jetpack_recharge_at < level_time {
        *jetpack_fuel += 1;
        packs.jetpack_recharge_at = level_time + REFUEL_RATE;
    }
    if cloaked {
        if packs.cloak_reduce_at < level_time {
            *cloak_fuel -= 1;
            if *cloak_fuel <= 0 {
                *cloak_fuel = 0;
                off = Toggled::Decloaked;
            }
            packs.cloak_reduce_at = level_time + DEFUEL_RATE;
        }
    } else if *cloak_fuel < 100 && packs.cloak_recharge_at < level_time {
        *cloak_fuel += 1;
        packs.cloak_recharge_at = level_time + REFUEL_RATE;
    }
    off
}
