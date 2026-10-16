//! The items a map places and what picking them up gives (OpenJK
//! `codemp/game/g_items.c` with `bg_misc.c`'s `bg_itemlist` and `BG_CanItemBeGrabbed`):
//! each dictionary with an item's classname becomes an entity dropped to the floor
//! (`FinishSpawningItem`), a trigger a player takes by coming near (`G_TouchTriggers`
//! with `BG_PlayerTouchesItem`'s generous box), which gives what it gives, goes away and
//! comes back on its own time (`RespawnItem`).

use crate::event_entity::EventEntity;
use crate::pmove::MovementTrace;
use sjk_entity::Entity;
use sjk_protocol::{EntityState, LEGACY_ENTITY_FIELDS, PlayerState};

/// `itemType_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `IT_BAD`: the list's first row, which is no item.
    Bad,
    Weapon,
    Ammo,
    Armor,
    Health,
    Powerup,
    Holdable,
    /// `IT_TEAM`: the flags, which belong to the team modes.
    Team,
}

/// A row of `bg_itemlist`: what a map's classname spawns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item {
    /// The classname a map spawns it by.
    pub classname: &'static str,
    /// `quantity`: how much it gives (a powerup's duration).
    pub quantity: i32,
    pub kind: Kind,
    /// `giTag`: the weapon, the ammo, the holdable, the powerup — or the armour's
    /// multiple of the maximum health.
    pub tag: i32,
}

const fn item(classname: &'static str, quantity: i32, kind: Kind, tag: i32) -> Item {
    Item {
        classname,
        quantity,
        kind,
        tag,
    }
}

/// `bg_itemlist` (`bg_misc.c:710-1685`): the list every wire `modelindex` indexes.
/// Index zero is no item; `bg_numItems` is the length less the list's terminator, so
/// every index from 1 to 50 is an item.
pub const ITEMS: [Item; 51] = [
    item("", 0, Kind::Bad, 0),
    item("item_shield_sm_instant", 25, Kind::Armor, 1),
    item("item_shield_lrg_instant", 100, Kind::Armor, 2),
    item("item_medpak_instant", 25, Kind::Health, 0),
    item("item_seeker", 120, Kind::Holdable, 1),
    item("item_shield", 120, Kind::Holdable, 2),
    item("item_medpac", 25, Kind::Holdable, 3),
    item("item_medpac_big", 25, Kind::Holdable, 4),
    item("item_binoculars", 60, Kind::Holdable, 5),
    item("item_sentry_gun", 120, Kind::Holdable, 6),
    item("item_jetpack", 120, Kind::Holdable, 7),
    item("item_healthdisp", 120, Kind::Holdable, 8),
    item("item_ammodisp", 120, Kind::Holdable, 9),
    item("item_eweb_holdable", 120, Kind::Holdable, 10),
    item("item_cloak", 120, Kind::Holdable, 11),
    item("item_force_enlighten_light", 25, Kind::Powerup, 12),
    item("item_force_enlighten_dark", 25, Kind::Powerup, 13),
    item("item_force_boon", 25, Kind::Powerup, 14),
    item("item_ysalimari", 25, Kind::Powerup, 15),
    item("weapon_stun_baton", 100, Kind::Weapon, 1),
    item("weapon_melee", 100, Kind::Weapon, 2),
    item("weapon_saber", 100, Kind::Weapon, 3),
    // The pistol every player spawns with; a map may place it too.
    item("weapon_blaster_pistol", 100, Kind::Weapon, 4),
    item("weapon_concussion_rifle", 50, Kind::Weapon, 15),
    item("weapon_bryar_pistol", 100, Kind::Weapon, 16),
    item("weapon_blaster", 100, Kind::Weapon, 5),
    item("weapon_disruptor", 100, Kind::Weapon, 6),
    item("weapon_bowcaster", 100, Kind::Weapon, 7),
    item("weapon_repeater", 100, Kind::Weapon, 8),
    item("weapon_demp2", 100, Kind::Weapon, 9),
    item("weapon_flechette", 100, Kind::Weapon, 10),
    item("weapon_rocket_launcher", 3, Kind::Weapon, 11),
    item("ammo_thermal", 4, Kind::Ammo, 7),
    item("ammo_tripmine", 3, Kind::Ammo, 8),
    item("ammo_detpack", 3, Kind::Ammo, 9),
    item("weapon_thermal", 4, Kind::Weapon, 12),
    item("weapon_trip_mine", 3, Kind::Weapon, 13),
    item("weapon_det_pack", 3, Kind::Weapon, 14),
    item("weapon_emplaced", 50, Kind::Weapon, 17),
    item("weapon_turretwp", 50, Kind::Weapon, 18),
    item("ammo_force", 100, Kind::Ammo, 1),
    item("ammo_blaster", 100, Kind::Ammo, 2),
    item("ammo_powercell", 100, Kind::Ammo, 3),
    item("ammo_metallic_bolts", 100, Kind::Ammo, 4),
    item("ammo_rockets", 3, Kind::Ammo, 5),
    item("ammo_all", 0, Kind::Ammo, -1),
    item("team_CTF_redflag", 0, Kind::Team, 4),
    item("team_CTF_blueflag", 0, Kind::Team, 5),
    item("team_CTF_neutralflag", 0, Kind::Team, 6),
    item("item_redcube", 0, Kind::Team, 0),
    item("item_bluecube", 0, Kind::Team, 0),
];

/// `ammoData[].max` (`bg_weapons.c:378-430`): how much of each kind a player may hold.
const AMMO_MAX: [i32; 10] = [0, 100, 300, 300, 300, 25, 800, 10, 10, 10];
/// `RESPAWN_ARMOR`, `RESPAWN_HEALTH`, `RESPAWN_AMMO`, `RESPAWN_HOLDABLE`,
/// `RESPAWN_MEGAHEALTH`, `RESPAWN_POWERUP`; `g_weaponRespawn`'s default.
const RESPAWN_ARMOR: i32 = 20;
const RESPAWN_HEALTH: i32 = 30;
const RESPAWN_AMMO: i32 = 40;
const RESPAWN_HOLDABLE: i32 = 60;
const RESPAWN_MEGAHEALTH: i32 = 120;
const RESPAWN_POWERUP: i32 = 120;
pub const WEAPON_RESPAWN: i32 = 5;
/// `ITMSF_SUSPEND`: the item hangs where the map put it.
const ITMSF_SUSPEND: i32 = 1;
/// `ITMSF_ALLOWNPC`: NPCs may take the item.
const ITMSF_ALLOWNPC: i32 = 4;
/// The wire fields an item carries, `CONTENTS_TRIGGER`, `MASK_SOLID`, `ET_ITEM`,
/// `EF_NODRAW`, `EF_ITEMPLACEHOLDER`, `EV_ITEM_PICKUP`, `EV_ITEM_RESPAWN`.
const ES_TYPE: usize = 8;
const ES_EFLAGS: usize = 19;
const ES_MODEL: usize = 46;
const ES_MODEL2: usize = 41;
const ES_GROUND_ENTITY: usize = 22;
const ES_ORIGIN: [usize; 3] = [11, 12, 13];
const ES_POS_BASE: [usize; 3] = [2, 1, 4];
pub const CONTENTS_TRIGGER: u32 = 0x400;
const MASK_SOLID: u32 = 0x1;
const ET_ITEM: u32 = 2;
const EF_NODRAW: u32 = 1 << 8;
const EF_ITEMPLACEHOLDER: u32 = 1 << 23;
pub const EV_ITEM_PICKUP: u32 = 22;
pub const EV_ITEM_RESPAWN: u32 = 62;
/// The stats and the fields a pickup writes.
const STAT_HOLDABLE_ITEM: usize = 1;
const STAT_HOLDABLE_ITEMS: usize = 2;
const STAT_WEAPONS: usize = 4;
const STAT_ARMOR: usize = 5;
const STAT_MAX_HEALTH: usize = 8;
const PS_EVENT_SEQUENCE: usize = 19;
const PS_EVENTS: [usize; 2] = [27, 28];
const PS_EVENT_PARMS: [usize; 2] = [65, 60];

/// The index of the item a classname spawns.
pub fn find(classname: &str) -> Option<usize> {
    // `G_CallSpawn`: `strcmp` over every row but the first.
    ITEMS
        .iter()
        .skip(1)
        .position(|item| item.classname == classname)
        .map(|index| index + 1)
}

/// An item as the map placed it, before it is dropped to the floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// Its row of `bg_itemlist`, which is its wire `modelindex`.
    pub item: usize,
    pub origin: [f32; 3],
    /// `wait`: the respawn time the map asks for, overriding the item's own.
    pub wait: f32,
    /// `random`: how far the respawn wanders.
    pub random: f32,
    /// `spawnflags & ITMSF_SUSPEND`: it hangs where it was put.
    pub suspended: bool,
    /// `spawnflags & ITMSF_ALLOWNPC`: NPCs may take it (`Touch_Item`).
    pub allow_npc: bool,
}

/// Every item dictionary of a map, in the entity lump's order (`G_SpawnItem` through
/// `G_SpawnEntitiesFromString`). Team items belong to the team modes and are left to
/// them; everything else a deathmatch server places is here.
pub fn placed(entities: &[Entity]) -> Vec<Placed> {
    entities
        .iter()
        .filter_map(|entity| {
            let item = find(entity.classname()?)?;
            let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
            let number = |key: &str| {
                entity
                    .get(key)
                    .and_then(|text| text.trim().parse::<f32>().ok())
                    .unwrap_or(0.0)
            };
            Some(Placed {
                item,
                origin,
                wait: number("wait"),
                random: number("random"),
                suspended: number("spawnflags") as i32 & ITMSF_SUSPEND != 0,
                allow_npc: number("spawnflags") as i32 & ITMSF_ALLOWNPC != 0,
            })
        })
        .collect()
}

/// An item entity on the server.
#[derive(Clone, Debug, PartialEq)]
pub struct Pickup {
    /// Its wire state; the pool numbers it.
    pub state: EntityState,
    /// Which item it is (`bg_itemlist`'s row).
    pub item: usize,
    /// `r.currentOrigin`: where it came to rest.
    pub origin: [f32; 3],
    /// `r.mins`, `r.maxs`.
    pub bounds: ([f32; 3], [f32; 3]),
    /// `r.contents`: a trigger while it is there, nothing while it is gone.
    pub contents: u32,
    /// `nextthink` with `RespawnItem`; zero while it stands.
    pub respawn_at: i32,
    pub wait: f32,
    pub random: f32,
    /// A dropped item's part (`FL_DROPPED_ITEM`); `None` for one the map placed.
    pub dropped: Option<crate::dropped_items::Dropped>,
    /// `spawnflags & ITMSF_ALLOWNPC`: a placed item NPCs may take.
    pub allow_npc: bool,
}

/// `FinishSpawningItem` (`g_items.c:2814-3006`): the box of eight by sixteen, `ET_ITEM`
/// with the item's row as its model, a trigger; suspended it stays where it was put,
/// else it is dropped to the floor — a tenth up first, the box a tenth shorter, traced
/// 4096 down through the world (the ground remembered). `trace` answers as
/// `trap->Trace` with `MASK_SOLID` would; `None` where the item starts in a solid, which
/// the reference frees with a print.
/// `FinishSpawningItem`'s game-type filter (`g_items.c:2820-2922`): whether `item` is
/// freed rather than placed — powerups in siege and Jedi Master, ammunition and the
/// seeker, shield and sentry on a saber-only server (but in Jedi Master), enlightenment in
/// Holocron FFA, the Force powerups with the Force disabled, health, armour and medpacs in
/// the duels, and the flags outside the flag games.
pub fn removed_by_gametype(
    item: usize,
    gametype: i32,
    saber_only: bool,
    force_disabled: bool,
) -> bool {
    const GT_HOLOCRON: i32 = 1;
    const GT_JEDIMASTER: i32 = 2;
    const GT_DUEL: i32 = 3;
    const GT_POWERDUEL: i32 = 4;
    const GT_SIEGE: i32 = 7;
    const GT_CTF: i32 = 8;
    const GT_CTY: i32 = 9;
    let row = ITEMS[item];
    let (kind, tag) = (row.kind, row.tag);
    // `HI_SEEKER`, `HI_SHIELD`, `HI_MEDPAC`, `HI_MEDPAC_BIG`, `HI_SENTRY_GUN`;
    // `PW_REDFLAG`..`PW_NEUTRALFLAG`, `PW_FORCE_ENLIGHTENED_LIGHT`, `_DARK`, `PW_FORCE_BOON`.
    let enlightenment = kind == Kind::Powerup && matches!(tag, 12 | 13);
    (gametype == GT_SIEGE && kind == Kind::Powerup)
        || (gametype != GT_JEDIMASTER
            && saber_only
            && (kind == Kind::Ammo || (kind == Kind::Holdable && matches!(tag, 1 | 2 | 6))))
        || (gametype == GT_JEDIMASTER && kind == Kind::Powerup)
        || (gametype == GT_HOLOCRON && enlightenment)
        || (force_disabled && kind == Kind::Powerup && matches!(tag, 12..=14))
        || (matches!(gametype, GT_DUEL | GT_POWERDUEL)
            && (matches!(kind, Kind::Armor | Kind::Health)
                || (kind == Kind::Holdable && matches!(tag, 3 | 4))))
        || (!matches!(gametype, GT_CTF | GT_CTY) && kind == Kind::Team && matches!(tag, 4..=6))
}

pub fn finish_spawning(
    placed: &Placed,
    trace: &mut dyn FnMut([f32; 3], [f32; 3], [f32; 3], [f32; 3], u32) -> MovementTrace,
) -> Option<Pickup> {
    let mut state = EntityState::zero(0, &LEGACY_ENTITY_FIELDS);
    state.set_raw_field(ES_TYPE, ET_ITEM);
    state.set_raw_field(ES_MODEL, placed.item as u32);
    state.set_raw_field(ES_MODEL2, 0);
    let bounds = ([-8.0, -8.0, 0.0], [8.0, 8.0, 16.0]);
    // `s.origin` is the map's, lifted a tenth for the drop; only the trajectory follows
    // the item down (`G_SetOrigin` leaves `s.origin` alone).
    let mut spawn = placed.origin;
    let origin = if placed.suspended {
        placed.origin
    } else {
        spawn[2] += 0.1;
        let end = [spawn[0], spawn[1], spawn[2] - 4_096.0];
        let found = trace(
            spawn,
            bounds.0,
            [bounds.1[0], bounds.1[1], bounds.1[2] - 0.1],
            end,
            MASK_SOLID,
        );
        if found.start_solid {
            return None;
        }
        state.set_raw_field(ES_GROUND_ENTITY, u32::from(found.entity_number));
        found.end_position
    };
    for axis in 0..3 {
        state.set_raw_field(ES_POS_BASE[axis], origin[axis].to_bits());
        state.set_raw_field(ES_ORIGIN[axis], spawn[axis].to_bits());
    }
    Some(Pickup {
        state,
        item: placed.item,
        origin,
        bounds,
        contents: CONTENTS_TRIGGER,
        respawn_at: 0,
        wait: placed.wait,
        random: placed.random,
        dropped: None,
        allow_npc: placed.allow_npc,
    })
}

/// `ClearRegisteredItems` and `SaveRegisteredItems` (`:3033-3090`): the string at
/// `CS_ITEMS`, a character per item — the four a player always has (the stun baton, the
/// fists, the saber and the pistol) and everything the map placed.
pub fn registered(placed: &[Placed]) -> Vec<u8> {
    let mut string = vec![b'0'; ITEMS.len()];
    string[19..23].fill(b'1');
    for item in placed {
        string[item.item] = b'1';
    }
    string
}

/// `BG_PlayerTouchesItem` (`bg_misc.c:1894-1912`): an item is taken from further than it
/// stands — 44 ahead and 50 behind on x, 36 each way on y and z.
pub fn touches(player: [f32; 3], item: [f32; 3]) -> bool {
    !(player[0] - item[0] > 44.0
        || player[0] - item[0] < -50.0
        || player[1] - item[1] > 36.0
        || player[1] - item[1] < -36.0
        || player[2] - item[2] > 36.0
        || player[2] - item[2] < -36.0)
}

/// `BG_CanItemBeGrabbed` (`bg_misc.c:2068-2210`) for an ordinary deathmatch player: a
/// weapon it already has is left (the thrown ones go by their ammo), ammo and armour and
/// health only below their maxima (the small and mega healths up to twice the maximum),
/// a holdable it already holds is left; powerups are always taken, team items never
/// (the modes' to add). A player in a duel takes nothing.
pub fn can_be_grabbed(item: usize, state: &PlayerState, dropped: bool) -> bool {
    let row = ITEMS[item];
    if state.raw_field(PS_DUEL_IN_PROGRESS).unwrap_or(0) != 0 {
        return false;
    }
    match row.kind {
        Kind::Weapon => {
            let thrown = matches!(row.tag, 12 | 13 | 14);
            if !dropped && !thrown && state.stats[STAT_WEAPONS] & (1 << row.tag) != 0 {
                return false;
            }
            if thrown {
                let ammo = crate::weapon_data::legacy_weapon_data(row.tag as u8)
                    .map_or(0, |data| data.ammo_index);
                return (state.ammo[ammo] as i32) < AMMO_MAX[ammo];
            }
            true
        }
        Kind::Ammo => {
            if row.tag == -1 {
                return true;
            }
            (state.ammo[row.tag as usize] as i32) < AMMO_MAX[row.tag as usize]
        }
        Kind::Armor => (state.stats[STAT_ARMOR] as i32) < state.stats[STAT_MAX_HEALTH] as i32,
        Kind::Health => {
            if state.force_powers_active() & FP_RAGE != 0 {
                return false;
            }
            let maximum = if row.quantity == 5 || row.quantity == 100 {
                state.stats[STAT_MAX_HEALTH] as i32 * 2
            } else {
                state.stats[STAT_MAX_HEALTH] as i32
            };
            (state.stats[0] as i32) < maximum
        }
        Kind::Powerup => true,
        Kind::Holdable => state.stats[STAT_HOLDABLE_ITEMS] & (1 << row.tag) == 0,
        Kind::Team | Kind::Bad => false,
    }
}

/// `ps.duelInProgress`; `FP_RAGE`'s bit in `fd.forcePowersActive`.
const PS_DUEL_IN_PROGRESS: usize = 119;
const FP_RAGE: u32 = 1 << 6;

/// `Add_Ammo`: up to the kind's maximum, never taking any away.
fn add_ammo(state: &mut PlayerState, kind: usize, count: i32) {
    let maximum = AMMO_MAX[kind];
    let held = state.ammo[kind] as i32;
    if held < maximum {
        state.ammo[kind] = (held + count).min(maximum) as u32;
    }
}

/// What a pickup did.
#[derive(Clone, Debug, PartialEq)]
pub struct Taken {
    /// When it comes again, in seconds; zero leaves it gone for good.
    pub respawn: i32,
    /// The pickup is one the player's client predicts (`EV_ITEM_PICKUP` on its own
    /// state) rather than being told of (the event on its entity): everything but a
    /// powerup, for a client that asked for it (`cg_predictItems`, which the reference
    /// takes from the userinfo — every client here asks).
    pub predicted: bool,
}

/// `Touch_Item`'s pickup (`g_items.c:2560-2578` and the `Pickup_*` of `:2052-2320`) on a
/// player that may take it: what it gives, and the seconds until it comes again
/// (`adjustRespawnTime` with `g_adaptRespawn` off — the default: the thrown weapons take
/// the ammo time). `health` is the entity's, which the stat follows.
pub fn pick_up(
    item: usize,
    state: &mut PlayerState,
    health: &mut i32,
    count: i32,
    dropped: bool,
) -> Taken {
    let row = ITEMS[item];
    let quantity = if count != 0 { count } else { row.quantity };
    match row.kind {
        Kind::Armor => {
            let maximum = state.stats[STAT_MAX_HEALTH] as i32 * row.tag;
            state.stats[STAT_ARMOR] =
                ((state.stats[STAT_ARMOR] as i32 + row.quantity).min(maximum)) as u32;
            Taken {
                respawn: RESPAWN_ARMOR,
                predicted: true,
            }
        }
        Kind::Health => {
            let maximum = if row.quantity != 5 && row.quantity != 100 {
                state.stats[STAT_MAX_HEALTH] as i32
            } else {
                state.stats[STAT_MAX_HEALTH] as i32 * 2
            };
            *health = (*health + quantity).min(maximum);
            state.stats[0] = *health as u32;
            Taken {
                respawn: if row.quantity == 100 {
                    RESPAWN_MEGAHEALTH
                } else {
                    RESPAWN_HEALTH
                },
                predicted: true,
            }
        }
        Kind::Ammo => {
            if row.tag == -1 {
                // `ammo_all` outside Siege: a little of everything.
                for (kind, count) in [(2, 50), (3, 50), (4, 50), (5, 2)] {
                    add_ammo(state, kind, count);
                }
            } else {
                add_ammo(state, row.tag as usize, quantity);
            }
            Taken {
                respawn: RESPAWN_AMMO,
                predicted: true,
            }
        }
        Kind::Weapon => {
            let ammo = crate::weapon_data::legacy_weapon_data(row.tag as u8)
                .map_or(0, |data| data.ammo_index);
            let mut quantity = quantity;
            if count < 0 {
                quantity = 0;
            } else if !dropped {
                // Under half the item's worth fills it up; over it, half is added.
                let held = state.ammo[ammo] as i32;
                quantity = if (held as f32) < quantity as f32 * 0.5 {
                    quantity - held
                } else {
                    (quantity as f32 * 0.5) as i32
                };
            }
            state.stats[STAT_WEAPONS] |= 1 << row.tag;
            add_ammo(state, ammo, quantity);
            let respawn = if matches!(row.tag, 12 | 13 | 14) {
                RESPAWN_AMMO
            } else {
                WEAPON_RESPAWN
            };
            Taken {
                respawn,
                predicted: true,
            }
        }
        Kind::Holdable => {
            state.stats[STAT_HOLDABLE_ITEM] = item as u32;
            state.stats[STAT_HOLDABLE_ITEMS] |= 1 << row.tag;
            Taken {
                respawn: RESPAWN_HOLDABLE,
                predicted: true,
            }
        }
        Kind::Powerup => Taken {
            respawn: RESPAWN_POWERUP,
            predicted: false,
        },
        Kind::Team | Kind::Bad => Taken {
            respawn: 0,
            predicted: true,
        },
    }
}

/// `BG_AddPredictableEventToPlayerstate`: the event on the player's own state, which its
/// client predicted (`EV_ITEM_PICKUP` with the item entity's number).
pub fn add_predictable_event(state: &mut PlayerState, event: u32, parameter: u32) {
    let sequence = state.raw_field(PS_EVENT_SEQUENCE).unwrap_or(0);
    let slot = (sequence & 1) as usize;
    state.set_raw_field(PS_EVENTS[slot], event);
    state.set_raw_field(PS_EVENT_PARMS[slot], parameter);
    state.set_raw_field(PS_EVENT_SEQUENCE, sequence.wrapping_add(1));
}

/// What `Touch_Item`'s tail did with the item (`:2605-2647`).
#[derive(Clone, Debug, PartialEq)]
pub enum Gone {
    /// Taken: it is out of the world until `respawn_at` (a weapon or a powerup leaves a
    /// placeholder behind, everything else is undrawn), or for good.
    Taken,
    /// `wait` of -1: taken once and never again.
    Forever,
}

/// The item after a pickup: no contents, undrawn — a weapon or a powerup that the map
/// placed stays drawn as a placeholder — and its think set to bring it back, the map's
/// `wait` overriding the item's own time and `random` wandering it (never under a
/// second). `spread` is `Q_flrand(-1, 1)`.
pub fn taken(pickup: &mut Pickup, respawn: i32, level_time: i32, spread: f32) -> Gone {
    let row = ITEMS[pickup.item];
    let mut respawn = respawn;
    if pickup.wait == -1.0 {
        pickup.contents = 0;
        pickup.state.set_raw_field(
            ES_EFLAGS,
            pickup.state.raw_field(ES_EFLAGS).unwrap_or(0) | EF_NODRAW,
        );
        pickup.respawn_at = 0;
        return Gone::Forever;
    }
    if pickup.wait != 0.0 {
        respawn = pickup.wait as i32;
    }
    if pickup.random != 0.0 {
        respawn += (spread * pickup.random) as i32;
        respawn = respawn.max(1);
    }
    let flags = pickup.state.raw_field(ES_EFLAGS).unwrap_or(0);
    if matches!(row.kind, Kind::Weapon | Kind::Powerup) {
        pickup
            .state
            .set_raw_field(ES_EFLAGS, (flags | EF_ITEMPLACEHOLDER) & !EF_NODRAW);
    } else {
        pickup.state.set_raw_field(ES_EFLAGS, flags | EF_NODRAW);
    }
    pickup.contents = 0;
    pickup.respawn_at = if respawn <= 0 {
        0
    } else {
        level_time + respawn * 1_000
    };
    Gone::Taken
}

/// `RespawnItem` (`:2323-2369`): a trigger again, drawn again, with `EV_ITEM_RESPAWN` on
/// itself; a powerup's return is heard by everyone (not ported: no powerups yet).
pub fn respawn(pickup: &mut Pickup, level_time: i32) -> EventEntity {
    pickup.contents = CONTENTS_TRIGGER;
    let flags = pickup.state.raw_field(ES_EFLAGS).unwrap_or(0);
    pickup
        .state
        .set_raw_field(ES_EFLAGS, flags & !(EF_NODRAW | EF_ITEMPLACEHOLDER));
    pickup.respawn_at = 0;
    let _ = level_time;
    EventEntity {
        event: EV_ITEM_RESPAWN,
        parameter: 0,
        origin: pickup.origin,
        client: None,
        broadcast: false,
        extra: [(0, 0); 12],
    }
}
