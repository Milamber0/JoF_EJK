//! The brushes a map puts there to be broken (OpenJK `codemp/game/g_mover.c`'s
//! `func_breakable` family): crates, glass, stone and the rest of the scenery a player
//! shoots apart.
//!
//! A breakable is the one brush entity that is *alive* — it has health, it takes damage
//! like a player, it answers each hit with a pain and, when its health runs out, it
//! clears itself out of the world, fires what it targets, throws its debris and its
//! explosion, hurts whoever stands too close if the map asked for that, and frees itself
//! a frame later.
//!
//! Two things about it are easy to get wrong and are ported here deliberately:
//!
//! * **Some weapons can never break one.** `G_Damage` (`g_combat.c:4525-4537`) refuses a
//!   `FL_BBRUSH` target outright for the Bryar pistol (either fire), the DEMP2 (either
//!   fire) and melee — "these don't damage bbrushes.. ever" — unless the melee comes from
//!   a class with heavy melee. A pistol bolt lands on a crate and does nothing at all.
//! * **`delay` is an integer key** (`g_spawn.c:138`, `F_INT`), so a map that writes
//!   `"delay" "0.5"` gets no delay whatever; only whole seconds count.

use sjk_entity::Entity;

/// `FL_BBRUSH`: I am a breakable brush, which is what `G_Damage` looks for.
pub const FL_BBRUSH: u32 = 0x0400_0000;
/// `FL_DMG_BY_SABER_ONLY` and `FL_DMG_BY_HEAVY_WEAP_ONLY`, the two the spawnflags set.
pub const FL_DMG_BY_SABER_ONLY: u32 = 0x0100_0000;
pub const FL_DMG_BY_HEAVY_WEAP_ONLY: u32 = 0x0200_0000;

/// `ET_MOVER`, which is what a breakable is on the wire.
pub const ET_MOVER: u32 = 6;
/// `CONTENTS_SOLID`.
pub const CONTENTS_SOLID: u32 = 1;
/// `SVF_PLAYER_USABLE`, which the PLAYER_USE spawnflag sets.
pub const SVF_PLAYER_USABLE: u32 = 0x0010;

/// `SP_func_breakable`'s spawnflags, as its own QUAKED comment names them.
const INVINCIBLE: u32 = 1;
const SABER_ONLY: u32 = 16;
const HEAVY_WEAP: u32 = 32;
const USE_NOT_BREAK: u32 = 64;
const PLAYER_USE: u32 = 128;
const NO_EXPLOSION: u32 = 2048;

/// `material_t` (`q_shared.h:503-523`), which decides what a break throws.
pub const MAT_METAL: u32 = 0;
pub const MAT_GLASS: u32 = 1;
pub const MAT_DRK_STONE: u32 = 4;
pub const MAT_LT_STONE: u32 = 5;
pub const MAT_GREY_STONE: u32 = 9;
pub const MAT_CRATE1: u32 = 11;
pub const MAT_SNOWY_ROCK: u32 = 16;

/// The four `meansOfDeath` a breakable brush simply ignores, plus melee
/// (`g_combat.c:4525-4537`). Melee is the fifth and is handled apart, because a class
/// with heavy melee may break one with its fists after all. `MOD_SABER` is the one a
/// saber-only brush wants.
pub use crate::means_of_death::{
    MOD_BRYAR_PISTOL, MOD_BRYAR_PISTOL_ALT, MOD_DEMP2, MOD_DEMP2_ALT, MOD_MELEE, MOD_SABER,
};

/// What the brush is waiting to do (`ent->think`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Thinking {
    /// Nothing.
    None,
    /// `funcBBrushDieGo`: the map asked for a delay before it comes apart.
    Breaking,
    /// `G_FreeEntity`: it has come apart and goes away next frame.
    Freeing,
}

/// A `func_breakable` as `SP_func_breakable` over `InitBBrush` leaves it.
#[derive(Clone, Debug, PartialEq)]
pub struct Breakable {
    /// The inline model it is made of, and that model's box.
    pub model: usize,
    pub bounds: ([f32; 3], [f32; 3]),
    /// What is left of it, and what the HUD is told (`showhealth`).
    pub health: i32,
    pub max_health: i32,
    pub takes_damage: bool,
    /// `ent->flags`: `FL_BBRUSH` and whichever of the two weapon restrictions apply.
    pub flags: u32,
    pub spawnflags: u32,
    pub contents: u32,
    pub svflags: u32,
    /// `material`: what its debris is made of.
    pub material: u32,
    /// `radius` scales how many chunks it throws, `mass` how big they are.
    pub radius: f32,
    pub mass: f32,
    /// What it does to whoever is standing close when it goes.
    pub splash_damage: i32,
    pub splash_radius: i32,
    /// `delay` in whole seconds before it actually comes apart, and `wait` as the
    /// milliseconds between two pains.
    pub delay: i32,
    pub wait: i32,
    /// `painDebounceTime`.
    pub pain_debounce: i32,
    /// What it is called, what it fires when it goes, and what it fires while it is hurt.
    pub targetname: String,
    pub target: String,
    pub paintarget: String,
    /// `playfx`, the effect a map may name for its death.
    pub effect: String,
    /// `nextthink` and `think`.
    pub next_think: i32,
    pub thinking: Thinking,
}

/// `SP_func_breakable` (`g_mover.c:2751-2849`) with `InitBBrush` (`:2619-2680`): ten
/// health unless the map says otherwise or marks it INVINCIBLE, the two weapon
/// restrictions its spawnflags ask for, the chunk scaling, and the brush model's own box
/// — solid, an `ET_MOVER` on the wire, and still.
pub fn spawn(entity: &Entity, bounds: ([f32; 3], [f32; 3])) -> Option<Breakable> {
    if entity.get("classname") != Some("func_breakable") {
        return None;
    }
    let model = entity
        .get("model")
        .and_then(|name| name.strip_prefix('*'))
        .and_then(|index| index.parse().ok())?;
    let float = |key: &str| {
        entity
            .get(key)
            .and_then(|text| text.trim().parse::<f32>().ok())
    };
    // `F_INT` keys: the spawn table truncates, so "0.5" is nothing at all.
    let integer = |key: &str| {
        entity
            .get(key)
            .and_then(|text| text.trim().parse::<i32>().ok())
    };
    let spawnflags = integer("spawnflags").unwrap_or(0) as u32;
    // `if (!(spawnflags & 1)) { if (!health) health = 10; }`: an INVINCIBLE brush is left
    // with whatever health the map gave it, which is usually none.
    let mut health = integer("health").unwrap_or(0);
    if spawnflags & INVINCIBLE == 0 && health == 0 {
        health = 10;
    }
    // `showhealth`: a non-zero maxHealth is what puts the bar on a client's HUD.
    let max_health = if integer("showhealth").unwrap_or(0) != 0 {
        health
    } else {
        0
    };
    let mut flags = FL_BBRUSH;
    if spawnflags & SABER_ONLY != 0 {
        flags |= FL_DMG_BY_SABER_ONLY;
    } else if spawnflags & HEAVY_WEAP != 0 {
        flags |= FL_DMG_BY_HEAVY_WEAP_ONLY;
    }
    let mut svflags = 0;
    if spawnflags & PLAYER_USE != 0 {
        svflags |= SVF_PLAYER_USABLE;
    }
    Some(Breakable {
        model,
        bounds,
        health,
        max_health,
        // `if (self->health) self->takedamage = qtrue;`
        takes_damage: health != 0,
        flags,
        spawnflags,
        contents: CONTENTS_SOLID,
        svflags,
        material: integer("material").unwrap_or(0) as u32,
        // Both default to one, and a zero is read back as one by the tail of the spawn.
        radius: match float("radius").unwrap_or(1.0) {
            0.0 => 1.0,
            radius => radius,
        },
        mass: match float("mass").unwrap_or(0.0) {
            0.0 => 1.0,
            mass => mass,
        },
        splash_damage: integer("splashDamage").unwrap_or(0),
        splash_radius: integer("splashRadius").unwrap_or(0),
        delay: integer("delay").unwrap_or(0),
        wait: integer("wait").unwrap_or(0),
        pain_debounce: 0,
        targetname: entity.get("targetname").unwrap_or_default().to_owned(),
        target: entity.get("target").unwrap_or_default().to_owned(),
        paintarget: entity.get("paintarget").unwrap_or_default().to_owned(),
        effect: entity.get("playfx").unwrap_or_default().to_owned(),
        next_think: 0,
        thinking: Thinking::None,
    })
}

/// The gates `G_Damage` puts in front of a breakable brush (`g_combat.c:4462-4537`):
/// whether this `means` may hurt it at all. `heavy_melee` is the attacker's own class
/// ability, which lets fists break a brush that otherwise wants explosives.
pub fn may_hurt(brush: &Breakable, means: u32, heavy_melee: bool) -> bool {
    if !brush.takes_damage {
        return false;
    }
    if brush.flags & FL_DMG_BY_SABER_ONLY != 0 && means != MOD_SABER {
        return false;
    }
    if brush.flags & FL_DMG_BY_HEAVY_WEAP_ONLY != 0
        && !heavy_weapon(means)
        && !(means == MOD_MELEE && heavy_melee)
    {
        return false;
    }
    // "these don't damage bbrushes.. ever"
    if matches!(
        means,
        MOD_BRYAR_PISTOL | MOD_BRYAR_PISTOL_ALT | MOD_DEMP2 | MOD_DEMP2_ALT
    ) {
        return false;
    }
    if means == MOD_MELEE && !heavy_melee {
        return false;
    }
    true
}

/// The `meansOfDeath` a `FL_DMG_BY_HEAVY_WEAP_ONLY` brush accepts (`:4496-4522`).
fn heavy_weapon(means: u32) -> bool {
    use crate::means_of_death::{
        MOD_CONC, MOD_CONC_ALT, MOD_CRUSH, MOD_DET_PACK_SPLASH, MOD_FALLING,
        MOD_FLECHETTE_ALT_SPLASH, MOD_REPEATER_ALT, MOD_ROCKET, MOD_ROCKET_HOMING, MOD_SUICIDE,
        MOD_TELEFRAG, MOD_THERMAL, MOD_THERMAL_SPLASH, MOD_TIMED_MINE_SPLASH, MOD_TRIGGER_HURT,
        MOD_TRIP_MINE_SPLASH, MOD_TURBLAST, MOD_VEHICLE,
    };
    matches!(
        means,
        MOD_REPEATER_ALT
            | MOD_ROCKET
            | MOD_FLECHETTE_ALT_SPLASH
            | MOD_ROCKET_HOMING
            | MOD_THERMAL
            | MOD_THERMAL_SPLASH
            | MOD_TRIP_MINE_SPLASH
            | MOD_TIMED_MINE_SPLASH
            | MOD_DET_PACK_SPLASH
            | MOD_VEHICLE
            | MOD_CONC
            | MOD_CONC_ALT
            | MOD_SABER
            | MOD_TURBLAST
            | MOD_SUICIDE
            | MOD_FALLING
            | MOD_CRUSH
            | MOD_TELEFRAG
            | MOD_TRIGGER_HURT
    )
}

/// `EV_DEBRIS` and `EV_MISC_MODEL_EXP`, and the `ET_EVENTS` a temp entity carries them
/// on the wire as (`eType = ET_EVENTS + event`, which for these two is 100 and 101).
pub const EV_DEBRIS: u32 = 82;
pub const EV_MISC_MODEL_EXP: u32 = 83;
pub const EV_GENERAL_SOUND: u32 = 76;
pub const ET_EVENTS: u32 = 18;

/// `r.absmin`/`r.absmax`: the box a linked entity wears, which is its model's grown by a
/// unit on every side (`SV_LinkEntity`). Every number `funcBBrushDieGo` and
/// `funcBBrushPain` work from is this one, not the model's own.
pub fn linked_box(brush: &Breakable) -> ([f32; 3], [f32; 3]) {
    (
        std::array::from_fn(|axis| brush.bounds.0[axis] - 1.0),
        std::array::from_fn(|axis| brush.bounds.1[axis] + 1.0),
    )
}

/// `G_Chunks` (`g_mover.c:2399-2416`): the debris a break throws, crammed whole into one
/// event entity's state — which is exactly how the reference sends it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Debris {
    /// `s.owner`: whose break this was.
    pub owner: u16,
    /// `s.origin`/`s.pos.trBase`: the middle of the box it came from.
    pub origin: [f32; 3],
    /// `s.angles`: the direction the chunks are thrown in.
    pub direction: [f32; 3],
    /// `s.origin2` and `s.angles2`: the box itself.
    pub maximums: [f32; 3],
    pub minimums: [f32; 3],
    /// `s.speed`, `s.eventParm` and `s.trickedentindex`.
    pub speed: f32,
    pub chunks: i32,
    pub material: u32,
    /// `s.modelindex`, a chunk model a map may name instead.
    pub custom: u32,
    /// `s.apos.trBase[0]`: how big each chunk is.
    pub scale: f32,
}

/// `G_MiscModelExplosion` (`:2383-2397`): the fireball, on its own event entity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Explosion {
    /// The middle of the box, which is where the event is placed.
    pub origin: [f32; 3],
    pub maximums: [f32; 3],
    pub minimums: [f32; 3],
    /// `s.time`: 0, 1 or 2 by how big the thing was.
    pub size: i32,
    /// `s.eventParm`.
    pub material: u32,
}

/// What a hit did to a brush (`G_Damage`'s tail with `funcBBrushPain`).
#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    /// The gates turned it away; nothing happened at all.
    Ignored,
    /// It lost health and is still standing. Carries what its pain did: the name it
    /// fires and the chunks a stone brush sheds on every hit.
    Hurt {
        paintarget: Option<String>,
        debris: Option<Debris>,
    },
    /// Its health ran out. `at` is when `funcBBrushDieGo` runs — now, or the map's own
    /// whole seconds later.
    Broken { at: i32 },
}

/// `G_Damage`'s tail for a breakable brush with `funcBBrushPain` (`:2552-2617`) and
/// `funcBBrushDie` (`:2520-2534`): the health comes off, a brush still standing answers
/// with its pain — at most one every `wait`, and a `wait` of -1 means one pain ever — and
/// one whose health has run out stops taking damage and starts coming apart.
///
/// `attacker` is where the chunks are thrown from, and `None` for anything that is not a
/// player (the reference then throws them straight up).
pub fn hurt(
    brush: &mut Breakable,
    damage: i32,
    means: u32,
    heavy_melee: bool,
    attacker: Option<[f32; 3]>,
    level_time: i32,
) -> Hit {
    if !may_hurt(brush, means, heavy_melee) {
        return Hit::Ignored;
    }
    brush.health -= damage;
    if brush.health <= 0 {
        // `funcBBrushDie`: no more damage, so a chain reaction cannot run away.
        brush.takes_damage = false;
        let at = level_time + brush.delay * 1_000;
        if brush.delay != 0 {
            brush.thinking = Thinking::Breaking;
            brush.next_think = at;
        }
        return Hit::Broken { at };
    }
    if brush.pain_debounce > level_time {
        return Hit::Hurt {
            paintarget: None,
            debris: None,
        };
    }
    let paintarget = (!brush.paintarget.is_empty()).then(|| brush.paintarget.clone());
    // Stone sheds chunks on every hit; nothing else does.
    let debris = matches!(
        brush.material,
        MAT_DRK_STONE | MAT_LT_STONE | MAT_GREY_STONE | MAT_SNOWY_ROCK
    )
    .then(|| {
        let (low, high) = linked_box(brush);
        let size: [f32; 3] = std::array::from_fn(|axis| high[axis] - low[axis]);
        let scale = (size[0] * size[0] + size[1] * size[1] + size[2] * size[2]).sqrt() / 100.0;
        let middle: [f32; 3] = std::array::from_fn(|axis| (low[axis] + high[axis]) * 0.5);
        // A pain throws its chunks *towards* whoever hit it, which is the other way round
        // from the death's.
        let direction = match attacker {
            Some(from) => {
                let mut away: [f32; 3] = std::array::from_fn(|axis| from[axis] - middle[axis]);
                normalize(&mut away);
                away
            }
            None => [0.0, 0.0, 1.0],
        };
        Debris {
            owner: 0,
            origin: middle,
            direction,
            maximums: high,
            minimums: low,
            speed: 300.0,
            // `Q_irand(1, 3)`, which the caller draws; the count is scaled by `radius`.
            chunks: 0,
            material: brush.material,
            custom: 0,
            scale: scale * brush.mass,
        }
    });
    // `wait == -1`: it has one pain in it and no more.
    if brush.wait == -1 {
        brush.paintarget.clear();
        brush.wait = 0;
        brush.pain_debounce = i32::MAX;
    } else {
        brush.pain_debounce = level_time + brush.wait;
    }
    Hit::Hurt { paintarget, debris }
}

/// How many chunks a pain sheds, given the generator's own `Q_irand(1, 3)`
/// (`:2600-2606`): a map's `radius` scales it, rounded up.
pub fn pain_chunks(brush: &Breakable, drawn: i32) -> i32 {
    if brush.radius > 0.0 {
        (drawn as f32 * brush.radius).ceil() as i32
    } else {
        drawn
    }
}

/// What `funcBBrushDieGo` (`:2418-2517`) made of the brush coming apart.
#[derive(Clone, Debug, PartialEq)]
pub struct Broken {
    /// What it fires on the way out, if a player broke it.
    pub target: Option<String>,
    /// The fireball, unless the map turned it off (NO_EXPLOSION).
    pub explosion: Option<Explosion>,
    /// The debris, always.
    pub debris: Debris,
    /// `splashDamage` over `splashRadius`, if the map asked for it — and the sound that
    /// goes with it.
    pub splash: Option<(i32, i32, [f32; 3])>,
    /// `playfx`, if the map named one.
    pub effect: Option<String>,
    /// When it frees itself (`level.time + 50`).
    pub freed_at: i32,
}

/// `funcBBrushDieGo` (`:2418-2517`): the brush stops being in the world at all, fires
/// what it targets, throws its debris and its fireball, hurts whoever is close if the map
/// asked, and frees itself fifty milliseconds on. `drawn` is `Q_flrand(0,1)` from the
/// game's own generator, which decides how many chunks there are.
pub fn break_apart(
    brush: &mut Breakable,
    attacker: Option<[f32; 3]>,
    number: u16,
    drawn: f32,
    level_time: i32,
) -> Broken {
    // So the chunks do not get stuck inside it.
    brush.contents = 0;
    brush.thinking = Thinking::Freeing;
    brush.next_think = level_time + 50;
    let (low, high) = linked_box(brush);
    let size: [f32; 3] = std::array::from_fn(|axis| high[axis] - low[axis]);
    let chunks = (drawn * 6.0) as i32 + 18;
    // "no logical basis other than ... the closest to yielding the results that I wanted"
    let mut scale = (size[0] * size[1] * size[2]).sqrt().sqrt() * 1.75;
    let size_class = if scale > 48.0 {
        2
    } else if scale > 24.0 {
        1
    } else {
        0
    };
    scale /= chunks as f32;
    let chunks = if brush.radius > 0.0 {
        (chunks as f32 * brush.radius) as i32
    } else {
        chunks
    };
    let middle: [f32; 3] = std::array::from_fn(|axis| (low[axis] + high[axis]) * 0.5);
    // A death throws its chunks *away* from whoever broke it.
    let direction = match attacker {
        Some(from) => {
            let mut away: [f32; 3] = std::array::from_fn(|axis| middle[axis] - from[axis]);
            normalize(&mut away);
            away
        }
        None => [0.0, 0.0, 1.0],
    };
    Broken {
        target: (attacker.is_some() && !brush.target.is_empty()).then(|| brush.target.clone()),
        explosion: (brush.spawnflags & NO_EXPLOSION == 0).then_some(Explosion {
            origin: middle,
            maximums: high,
            minimums: low,
            size: size_class,
            material: brush.material,
        }),
        debris: Debris {
            owner: number,
            origin: middle,
            direction,
            maximums: high,
            minimums: low,
            speed: 300.0,
            chunks,
            material: brush.material,
            custom: 0,
            scale: scale * brush.mass,
        },
        splash: (brush.splash_damage > 0 && brush.splash_radius > 0).then_some((
            brush.splash_damage,
            brush.splash_radius,
            middle,
        )),
        effect: (!brush.effect.is_empty()).then(|| brush.effect.clone()),
        freed_at: level_time + 50,
    }
}

/// `funcBBrushUse` (`:2536-2550`): the use key breaks it, unless the map said using it
/// only fires its targets. Returns what to fire when it is the latter.
pub fn used(brush: &mut Breakable, level_time: i32) -> Option<String> {
    if brush.spawnflags & USE_NOT_BREAK != 0 {
        return (!brush.target.is_empty()).then(|| brush.target.clone());
    }
    // `funcBBrushDie(self, other, activator, self->health, MOD_UNKNOWN)`: its own health
    // as the damage, so it always dies.
    brush.takes_damage = false;
    brush.health = 0;
    if brush.delay != 0 {
        brush.thinking = Thinking::Breaking;
        brush.next_think = level_time + brush.delay * 1_000;
    }
    None
}

/// `VectorNormalize`, which the reference does with `sqrtf` and one reciprocal.
fn normalize(vector: &mut [f32; 3]) -> f32 {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length != 0.0 {
        let scale = 1.0 / length;
        for axis in vector.iter_mut() {
            *axis *= scale;
        }
    }
    length
}

/// The wire state of an event entity carrying [`Debris`] or [`Explosion`], as
/// `G_TempEntity` plus the crammed fields leave it: `(field index, raw bits)` pairs.
pub fn debris_fields(debris: &Debris) -> Vec<(usize, u32)> {
    let mut fields = vec![
        (8, ET_EVENTS + EV_DEBRIS),
        (40, u32::from(debris.owner)),
        (31, debris.speed.to_bits()),
        (42, debris.chunks as u32),
        (58, debris.material),
        (46, debris.custom),
        (5, debris.scale.to_bits()),
    ];
    place(&mut fields, [2, 1, 4], debris.origin);
    place(&mut fields, [11, 12, 13], debris.origin);
    place(&mut fields, [25, 9, 24], debris.direction);
    place(&mut fields, [56, 60, 53], debris.maximums);
    place(&mut fields, [82, 51, 84], debris.minimums);
    fields.retain(|(_, bits)| *bits != 0);
    fields.sort_unstable();
    fields
}

/// The same for the fireball.
pub fn explosion_fields(explosion: &Explosion) -> Vec<(usize, u32)> {
    let mut fields = vec![
        (8, ET_EVENTS + EV_MISC_MODEL_EXP),
        (65, explosion.size as u32),
        (42, explosion.material),
    ];
    place(&mut fields, [2, 1, 4], explosion.origin);
    place(&mut fields, [56, 60, 53], explosion.maximums);
    place(&mut fields, [82, 51, 84], explosion.minimums);
    fields.retain(|(_, bits)| *bits != 0);
    fields.sort_unstable();
    fields
}

fn place(fields: &mut Vec<(usize, u32)>, indices: [usize; 3], vector: [f32; 3]) {
    for axis in 0..3 {
        fields.push((indices[axis], vector[axis].to_bits()));
    }
}
