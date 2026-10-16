//! Siege's carried and breakable objectives (`misc_siege_item`, `g_saga.c:1363-1936`) and
//! the physics a dropped one falls by (`G_RunExPhys`, `g_exphysics.c`).
//!
//! An item is picked up by walking into it (unless its `teamnotouch` side), rides on its
//! carrier (broadcast, bolted to the player), drops where the carrier dies and falls with a
//! random throw, goes home when left too long, and is delivered by walking into the trigger
//! its `goaltarget` names (that half is `siege_triggers`). One with health can be struck and
//! broken; one with a `targetname` appears where it is first used. What an item asks of the
//! level — traces, targets fired, sounds, effects, its carrier — goes through
//! [`SiegeItemWorld`]; the rules are here.

use sjk_entity::Entity;

/// `SIEGE_ITEM_RESPAWN_TIME`: how long a dropped item may lie before it goes home.
pub const SIEGE_ITEM_RESPAWN_TIME: i32 = 20_000;
/// `SIEGEITEM_STARTOFFRADAR` as `g_saga.c:40` redefines it.
pub const SIEGEITEM_STARTOFFRADAR: u32 = 8;
/// `ENTITYNUM_NONE`: carried by nobody.
pub const ENTITYNUM_NONE: u16 = 1_023;
/// `EF_RADAROBJECT`, `EF_NODRAW`, `EF_CLIENTSMOOTH`.
pub const EF_RADAROBJECT: u32 = 1 << 2;
pub const EF_NODRAW: u32 = 1 << 8;
pub const EF_CLIENTSMOOTH: u32 = 1 << 28;
/// `SVF_BROADCAST`.
pub const SVF_BROADCAST: u32 = 0x20;
/// The contents an item takes: a trigger to walk into, or as solid as a player.
pub const CONTENTS_SOLID: u32 = 0x1;
pub const CONTENTS_TRIGGER: u32 = 0x400;
pub const CONTENTS_NODROP: u32 = 0x800;
pub const CONTENTS_TERRAIN: u32 = 0x1000;
/// `MASK_PLAYERSOLID`.
pub const MASK_PLAYERSOLID: u32 = 0x1 | 0x10 | 0x100 | 0x1000;
/// `MAX_GRAVITY_PULL`: the most gravity an item gathers.
const MAX_GRAVITY_PULL: f32 = 512.0;
/// `MAX_CLIENTS`: a carrier below it is bolted to (`boltToPlayer`).
const MAX_CLIENTS: u16 = 32;

/// What an item does when used (`ent->use`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnUse {
    Nothing,
    /// `SiegeItemUse`.
    Activate,
}

/// One `misc_siege_item` as `SP_misc_siege_item` leaves it and its thinks change it; the
/// `genericValueN` it keeps are named for what they are.
#[derive(Clone, Debug, PartialEq)]
pub struct SiegeItem {
    pub model: String,
    pub targetname: Option<String>,
    /// `target2` (picked up), `target3` (delivered), `target4` (broken), `target5`
    /// (sent home), `target6` (dropped).
    pub target2: Option<String>,
    pub target3: Option<String>,
    pub target4: Option<String>,
    pub target5: Option<String>,
    pub target6: Option<String>,
    /// The trigger it is delivered to (`goaltarget`).
    pub goaltarget: Option<String>,
    /// Where it appears when used (`paintarget`).
    pub paintarget: Option<String>,
    /// `genericValue1`: it falls by [`run_ex_phys`].
    pub use_physics: bool,
    /// `genericValue2`: somebody has it.
    pub picked: bool,
    /// `genericValue3`, `genericValue10`: its death and respawn effects.
    pub death_fx: u16,
    pub respawn_fx: u16,
    /// `genericValue4`, `genericValue5`: `target2` only on the first pickup, and whether
    /// that was.
    pub pickup_only_once: bool,
    pub picked_before: bool,
    /// `genericValue6`, `genericValue7`: the side that may not touch it, the side that may
    /// not deliver it.
    pub team_no_touch: i32,
    pub team_no_complete: i32,
    /// `genericValue8`: its carrier, [`ENTITYNUM_NONE`] for nobody.
    pub carrier: u16,
    /// `genericValue9`: when a dropped one goes home, 0 for never.
    pub respawn_at: i32,
    /// `genericValue11`: a hidden one may be picked up once used.
    pub pickup_once_used: bool,
    /// `genericValue12`..`14`: health regained, how often, and when next.
    pub recharge: i32,
    pub recharge_rate: i32,
    pub recharge_at: i32,
    /// `genericValue15`: the carrier's Force is crippled (`forcelimit`).
    pub force_limit: bool,
    /// `noise_index`: the pickup sound.
    pub pickup_sound: u16,
    /// `mass`, `radius` (its gravity) and `random` (its bounce), for [`run_ex_phys`].
    pub mass: f32,
    pub gravity: f32,
    pub bounce: f32,
    /// `r.currentOrigin`, `pos1` (home), `r.currentAngles`.
    pub origin: [f32; 3],
    pub home: [f32; 3],
    pub angles: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub eflags: u32,
    pub svflags: u32,
    pub contents: u32,
    pub clipmask: u32,
    /// `s.time2`: a radar pulse (a time), a blink forever (-1) or nothing.
    pub time2: i32,
    /// `s.boltToPlayer`: the carrier's number plus one, for a player.
    pub bolt_to_player: u32,
    pub health: i32,
    pub max_health: i32,
    pub take_damage: bool,
    /// Whether it thinks (`think`), and whether walking into it is its touch.
    pub thinks: bool,
    pub touch: bool,
    pub on_use: OnUse,
    /// `SIEGEITEM_STARTOFFRADAR`: off the radar until used.
    pub start_off_radar: bool,
    /// `epVelocity`, `epGravFactor`, `s.groundEntityNum`.
    pub velocity: [f32; 3],
    pub grav_factor: f32,
    pub ground: u16,
    /// `s.genericenemyindex` (its radar icon), `s.modelindex`, `s.modelGhoul2`.
    pub icon: u16,
    pub model_index: u16,
    pub ghoul2: bool,
    /// `think == G_FreeEntity`: broken, it is freed at its next think.
    pub dying: bool,
    /// `G_FreeEntity`: gone (freed after breaking, or delivered).
    pub freed: bool,
}

/// Why a map's item cannot stand (the reference's `ERR_DROP`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// "You must specify a model for misc_siege_item types."
    NoModel,
}

/// The indices an item registers as it spawns (`G_SoundIndex`, `G_EffectIndex`,
/// `G_IconIndex`, `G_ModelIndex`), asked of the level in the reference's order.
pub trait Registries {
    fn sound(&mut self, name: &str) -> u16;
    fn effect(&mut self, name: &str) -> u16;
    fn icon(&mut self, name: &str) -> u16;
    fn model(&mut self, name: &str) -> u16;
}

/// `SP_misc_siege_item`: `None` outside a playable siege level (it is freed there).
pub fn spawn(
    entity: &Entity,
    siege: bool,
    registries: &mut dyn Registries,
) -> Option<Result<SiegeItem, Refused>> {
    if !siege {
        return None;
    }
    let text = |key: &str| {
        entity
            .get(key)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let int = |key: &str, default: i32| {
        entity
            .get(key)
            .map_or(default, |value| crate::userinfo::atoi(value.as_bytes()))
    };
    let float = |key: &str, default: f32| {
        entity
            .get(key)
            .map_or(default, |value| crate::text_parse::atof(value.as_bytes()))
    };
    let vector =
        |key: &str, default: [f32; 3]| entity.vector(key).ok().flatten().unwrap_or(default);
    let Some(model) = text("model") else {
        return Some(Err(Refused::NoModel));
    };
    let spawnflags = int("spawnflags", 0) as u32;
    let can_pickup = int("canpickup", 1) != 0;
    let use_physics = int("usephysics", 1) != 0;
    let mut eflags = if use_physics { EF_CLIENTSMOOTH } else { 0 };
    let no_radar = int("noradar", 0) != 0;
    if !no_radar && spawnflags & SIEGEITEM_STARTOFFRADAR == 0 {
        eflags |= EF_RADAROBJECT;
    }
    let pickup_only_once = int("pickuponlyonce", 1) != 0;
    let (team_no_touch, team_no_complete) = (int("teamnotouch", 0), int("teamnocomplete", 0));
    let (mass, gravity, bounce) = (
        float("mass", 0.09),
        float("gravity", 3.0),
        float("bounce", 1.3),
    );
    let pickup_sound = text("pickupsound").map_or(0, |name| registries.sound(&name));
    let death_fx = text("deathfx").map_or(0, |name| registries.effect(&name));
    let respawn_fx = text("respawnfx").map_or(0, |name| registries.effect(&name));
    let icon = text("icon").map_or(0, |name| registries.icon(&name));
    let model_index = registries.model(&model);
    let ghoul2 = model.len() >= 4 && model[model.len() - 4..].eq_ignore_ascii_case(".glm");
    let origin = vector("origin", [0.0; 3]);
    let angles = match entity.vector("angles").ok().flatten() {
        Some(angles) => angles,
        None => [0.0, float("angle", 0.0), 0.0],
    };
    let force_limit = int("forcelimit", 0) != 0;
    let health = int("health", 0);
    let (take_damage, max_health, recharge, recharge_rate) = if health > 0 {
        if int("showhealth", 0) != 0 {
            (
                true,
                health,
                int("health_chargeamt", 0),
                int("health_chargerate", 0),
            )
        } else {
            (true, 0, 0, 0)
        }
    } else {
        (false, 0, 0, 0)
    };
    let targetname = text("targetname");
    let mut item = SiegeItem {
        model,
        targetname: targetname.clone(),
        target2: text("target2"),
        target3: text("target3"),
        target4: text("target4"),
        target5: text("target5"),
        target6: text("target6"),
        goaltarget: text("goaltarget"),
        paintarget: text("paintarget"),
        use_physics,
        picked: false,
        death_fx,
        respawn_fx,
        pickup_only_once,
        picked_before: false,
        team_no_touch,
        team_no_complete,
        carrier: ENTITYNUM_NONE,
        respawn_at: 0,
        pickup_once_used: false,
        recharge,
        recharge_rate,
        recharge_at: 0,
        force_limit,
        pickup_sound,
        mass,
        gravity,
        bounce,
        origin,
        home: origin,
        angles,
        mins: vector("mins", [-16.0, -16.0, -24.0]),
        maxs: vector("maxs", [16.0, 16.0, 32.0]),
        eflags,
        svflags: SVF_BROADCAST,
        contents: 0,
        clipmask: 0,
        time2: 0,
        bolt_to_player: 0,
        health,
        max_health,
        take_damage,
        thinks: false,
        touch: false,
        on_use: OnUse::Nothing,
        start_off_radar: spawnflags & SIEGEITEM_STARTOFFRADAR != 0,
        velocity: [0.0; 3],
        grav_factor: 0.0,
        ground: 0,
        icon,
        model_index,
        ghoul2,
        dying: false,
        freed: false,
    };
    let start_off = spawnflags & SIEGEITEM_STARTOFFRADAR != 0;
    if start_off {
        item.on_use = OnUse::Activate;
    } else if targetname.is_some() {
        // "kind of hacky, but whatever": hidden until used.
        item.eflags |= EF_NODRAW;
        item.pickup_once_used = can_pickup;
        item.on_use = OnUse::Activate;
        item.eflags &= !EF_RADAROBJECT;
    }
    if targetname.is_none() || start_off {
        make_touchable(&mut item, can_pickup);
        item.thinks = true;
    }
    Some(Ok(item))
}

/// The contents and touch an item takes when it becomes active: a trigger to walk into
/// when it can be picked up (or cannot be hurt), else as solid as a player.
fn make_touchable(item: &mut SiegeItem, can_pickup: bool) {
    if can_pickup || !item.take_damage {
        item.contents = CONTENTS_TRIGGER;
        item.clipmask = CONTENTS_SOLID | CONTENTS_TERRAIN;
        if can_pickup {
            item.touch = true;
        }
    } else {
        item.contents = MASK_PLAYERSOLID;
        item.clipmask = MASK_PLAYERSOLID;
    }
}

/// A trace as the item's think reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trace {
    pub fraction: f32,
    pub end: [f32; 3],
    pub normal: [f32; 3],
    pub start_solid: bool,
    pub all_solid: bool,
    /// `entityNum`: what was struck, [`ENTITYNUM_NONE`] for nothing.
    pub entity: u16,
}

/// A carrier as the item's think reads it (`g_entities[genericValue8]`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CarrierView {
    /// `inuse && client`.
    pub in_game: bool,
    pub origin: [f32; 3],
    pub view_angles: [f32; 3],
    pub health: i32,
    /// `sess.sessionTeam`.
    pub team: i32,
    /// `PMF_FOLLOW`: it follows another player.
    pub following: bool,
}

/// What an item asks of the level.
pub trait SiegeItemWorld {
    /// `trap->Trace` of the item's box, passing the item itself (`pass`).
    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        pass: u16,
        mask: u32,
    ) -> Trace;
    /// `trap->PointContents(point, pass)`.
    fn point_contents(&mut self, point: [f32; 3], pass: u16) -> u32;
    /// The client (or other entity) numbered `number`, if it is one.
    fn carrier(&mut self, number: u16) -> Option<CarrierView>;
    /// `G_UseTargets2(item, item, name)`.
    fn use_targets(&mut self, item: u16, name: &str);
    /// `G_PlayEffectID(effect, origin, up)`.
    fn play_effect(&mut self, effect: u16, origin: [f32; 3]);
    /// `Q_irand(min, max)`, the game's generator.
    fn irand(&mut self, min: i32, max: i32) -> i32;
    /// The carrier lets go (`SiegeItemRemoveOwner`'s client half): `holdingObjectiveItem`
    /// 0, no longer broadcast.
    fn release(&mut self, carrier: u16);
}

/// `SiegeItemRemoveOwner`: nobody has it, and the carrier (if any) knows it.
pub fn remove_owner(item: &mut SiegeItem, world: &mut dyn SiegeItemWorld, carrier: Option<u16>) {
    item.picked = false;
    item.carrier = ENTITYNUM_NONE;
    if let Some(carrier) = carrier {
        world.release(carrier);
    }
}

/// `SiegeItemRespawnEffect`: `target5`, and the respawn effect at both ends.
fn respawn_effect(item: &SiegeItem, number: u16, world: &mut dyn SiegeItemWorld, to: [f32; 3]) {
    if let Some(target) = &item.target5 {
        world.use_targets(number, target);
    }
    if item.respawn_fx == 0 {
        return;
    }
    world.play_effect(item.respawn_fx, item.origin);
    world.play_effect(item.respawn_fx, to);
}

/// `SiegeItemRespawnOnOriginalSpot`.
fn respawn_home(
    item: &mut SiegeItem,
    number: u16,
    world: &mut dyn SiegeItemWorld,
    carrier: Option<u16>,
) {
    respawn_effect(item, number, world, item.home);
    item.origin = item.home;
    remove_owner(item, world, carrier);
    item.time2 = 0;
}

/// `SiegeItemThink` for item `number` at `level_time`.
pub fn think(item: &mut SiegeItem, number: u16, world: &mut dyn SiegeItemWorld, level_time: i32) {
    if item.dying {
        item.freed = true;
        return;
    }
    if item.recharge != 0
        && item.health > 0
        && item.health < item.max_health
        && item.recharge_at < level_time
    {
        item.recharge_at = level_time + item.recharge_rate;
        item.health = (item.health + item.recharge).min(item.max_health);
    }
    let mut carrier = None;
    if item.carrier != ENTITYNUM_NONE {
        let view = world.carrier(item.carrier);
        if let Some(view) = view.filter(|view| view.in_game) {
            item.origin = view.origin;
        }
        carrier = Some((item.carrier, view));
    } else if item.use_physics {
        run_ex_phys(item, number, world);
    }
    item.bolt_to_player = if item.carrier < MAX_CLIENTS {
        u32::from(item.carrier) + 1
    } else {
        0
    };
    if let Some((who, view)) = carrier {
        let gone = view.is_none_or(|view| {
            !view.in_game || (view.team != 1 && view.team != 2) || view.following
        });
        if gone {
            respawn_home(item, number, world, None);
        } else if let Some(view) = view.filter(|view| view.health < 1) {
            if let Some(target) = item.target6.clone() {
                world.use_targets(number, &target);
            }
            if world.point_contents(view.origin, who) & CONTENTS_NODROP != 0 {
                respawn_home(item, number, world, Some(who));
            } else {
                // A start-solid check, up a bit, then back from where the carrier faced.
                let (mins, maxs, mask) = (item.mins, item.maxs, item.clipmask);
                let mut point = view.origin;
                let trace = world.trace(point, mins, maxs, point, number, mask);
                if trace.start_solid {
                    point[2] += 30.0;
                    let trace = world.trace(point, mins, maxs, point, number, mask);
                    if trace.start_solid {
                        let forward = crate::npc_sniper::angle_vectors(view.view_angles).0;
                        point = std::array::from_fn(|axis| point[axis] + -30.0 * forward[axis]);
                        let trace = world.trace(point, mins, maxs, point, number, mask);
                        if trace.start_solid {
                            respawn_home(item, number, world, Some(who));
                            return;
                        }
                    }
                }
                item.origin = point;
                item.velocity[0] = world.irand(-80, 80) as f32;
                item.velocity[1] = world.irand(-80, 80) as f32;
                item.velocity[2] = world.irand(40, 80) as f32;
                item.respawn_at = level_time + SIEGE_ITEM_RESPAWN_TIME;
                remove_owner(item, world, Some(who));
            }
        }
    }
    if item.respawn_at != 0 && item.respawn_at < level_time {
        respawn_effect(item, number, world, item.home);
        item.origin = item.home;
        item.respawn_at = 0;
        item.time2 = 0;
    }
}

/// Who walks into an item, as `SiegeItemTouch` reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Toucher {
    pub number: u16,
    /// A client that is no NPC.
    pub player: bool,
    pub health: i32,
    /// `holdingObjectiveItem` is set.
    pub carrying: bool,
    /// `pm_type == PM_SPECTATOR`.
    pub spectator: bool,
    pub team: i32,
}

/// What a touch did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Touched {
    Nothing,
    /// Picked up by the toucher: its sound played on it where there is one, and `target2`
    /// to fire (set when it fires this time).
    PickedUp {
        sound: Option<u16>,
        fire: Option<String>,
    },
}

/// `SiegeItemTouch` by `who` (`None` for something that is no player, which only frees an
/// item stuck in a solid by a unit up).
pub fn touch(
    item: &mut SiegeItem,
    who: Option<Toucher>,
    round_begun: bool,
    start_solid: bool,
) -> Touched {
    let Some(who) = who.filter(|who| who.player) else {
        if start_solid {
            item.origin[2] += 1.0;
        }
        return Touched::Nothing;
    };
    if who.health < 1
        || who.carrying
        || who.spectator
        || item.picked
        || item.team_no_touch == who.team
        || !round_begun
    {
        return Touched::Nothing;
    }
    item.picked = true;
    item.carrier = who.number;
    item.respawn_at = 0;
    let fire = item
        .target2
        .clone()
        .filter(|_| !item.pickup_only_once || !item.picked_before);
    if fire.is_some() {
        item.picked_before = true;
    }
    item.time2 = -1;
    Touched::PickedUp {
        sound: (item.pickup_sound != 0).then_some(item.pickup_sound),
        fire,
    }
}

/// `SiegeItemPain`: the radar pulses.
pub fn pain(item: &mut SiegeItem, level_time: i32) {
    item.time2 = level_time;
}

/// `SiegeItemDie`: no more damage, the death effect where it stands, `target4`, and it is
/// freed at its next think (`think = G_FreeEntity`, `nextthink = level.time`).
pub fn die(item: &mut SiegeItem, number: u16, world: &mut dyn SiegeItemWorld) {
    item.take_damage = false;
    if item.death_fx != 0 {
        world.play_effect(item.death_fx, item.origin);
    }
    item.dying = true;
    if let Some(target) = item.target4.clone() {
        world.use_targets(number, &target);
    }
}

/// `SiegeItemUse`, with where its `paintarget` stands (`G_Find`'s first of that name) and
/// what that is.
pub fn use_item(
    item: &mut SiegeItem,
    number: u16,
    world: &mut dyn SiegeItemWorld,
    paint: Option<Painted>,
) {
    if item.start_off_radar {
        item.eflags |= EF_RADAROBJECT;
        if item.eflags & EF_NODRAW == 0 {
            return;
        }
    } else {
        item.eflags |= EF_RADAROBJECT;
    }
    make_touchable(item, item.pickup_once_used);
    item.thinks = true;
    item.eflags &= !EF_NODRAW;
    let Some(paint) = paint else { return };
    let (mins, maxs, mask) = (item.mins, item.maxs, item.clipmask);
    let mut point = paint.origin;
    let trace = world.trace(paint.origin, mins, maxs, paint.origin, paint.number, mask);
    if trace.start_solid {
        point[2] += 30.0;
        let trace = world.trace(point, mins, maxs, point, number, mask);
        if trace.start_solid {
            let forward = crate::npc_sniper::angle_vectors(paint.facing).0;
            point = std::array::from_fn(|axis| point[axis] + -30.0 * forward[axis]);
            let trace = world.trace(point, mins, maxs, point, number, mask);
            if trace.start_solid {
                return;
            }
        }
    }
    item.origin = point;
}

/// A `paintarget`: its number, where it stands, and the way it faces (a client's view, else
/// its angles).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Painted {
    pub number: u16,
    pub origin: [f32; 3],
    pub facing: [f32; 3],
}

/// `G_RunExPhys` with no Ghoul2 bolts and no auto-kill (`SiegeItemThink`'s own call):
/// gravity gathered while nothing is below, the velocity cut by the mass and moved along,
/// and a bounce off what it struck (its touch called first).
pub fn run_ex_phys(item: &mut SiegeItem, number: u16, world: &mut dyn SiegeItemWorld) {
    let (gravity, mass, bounce) = (item.gravity, item.mass, item.bounce);
    let (mins, maxs, mask) = (item.mins, item.maxs, item.clipmask);
    if gravity != 0.0 {
        let mut ground = item.origin;
        ground[2] -= 0.1;
        let trace = world.trace(item.origin, mins, maxs, ground, number, mask);
        item.ground = if trace.fraction == 1.0 {
            ENTITYNUM_NONE
        } else {
            trace.entity
        };
        if item.ground == ENTITYNUM_NONE {
            item.grav_factor += gravity;
            if item.grav_factor > MAX_GRAVITY_PULL {
                item.grav_factor = MAX_GRAVITY_PULL;
            }
            item.velocity[2] -= item.grav_factor;
        } else {
            item.grav_factor = 0.0;
        }
    }
    if item.velocity == [0.0; 3] {
        if item.touch {
            let trace = world.trace(item.origin, mins, maxs, item.origin, number, mask);
            if trace.start_solid || trace.all_solid {
                // The touch of whatever it is in: never a player here (their contents are
                // not in the item's mask), so only the way out of a solid.
                let _ = touch(item, None, false, trace.start_solid);
            }
        }
        return;
    }
    let projected: [f32; 3] =
        std::array::from_fn(|axis| item.origin[axis] + 0.1 * item.velocity[axis]);
    for axis in 0..3 {
        item.velocity[axis] *= 1.0 - mass;
    }
    let (_, mut total) = normalize(item.velocity);
    if total < 1.0 && item.ground != ENTITYNUM_NONE {
        item.velocity = [0.0; 3];
        item.grav_factor = 0.0;
        return;
    }
    let trace = world.trace(item.origin, mins, maxs, projected, number, mask);
    if trace.start_solid || trace.all_solid {
        return;
    }
    item.origin = trace.end;
    if trace.fraction == 1.0 {
        return;
    }
    if bounce != 0.0 {
        total *= bounce;
        let direction = trace.normal.map(|value| value * total);
        if direction[2] > 0.0 {
            item.grav_factor -= direction[2] * (1.0 - mass);
            if item.grav_factor < 0.0 {
                item.grav_factor = 0.0;
            }
        }
        if trace.entity != ENTITYNUM_NONE && item.touch {
            // Its touch on what it struck: the world or a thing, never a player (see above).
            let _ = touch(item, None, false, trace.start_solid);
        }
        for axis in 0..3 {
            item.velocity[axis] += direction[axis];
        }
    } else {
        item.velocity[0] = 0.0;
        item.velocity[1] = 0.0;
        if gravity == 0.0 {
            item.velocity[2] = 0.0;
        }
    }
}

/// `VectorNormalize` on a copy: the unit vector and the length, in the game's float
/// arithmetic (`sqrtf`, `1/length`).
fn normalize(vector: [f32; 3]) -> ([f32; 3], f32) {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length == 0.0 {
        return (vector, length);
    }
    let inverse = 1.0 / length;
    (vector.map(|value| value * inverse), length)
}

/// `G_PlayEffectID(effect, origin, (0 0 1))`: an item's death or respawn effect.
pub fn effect_event(effect: u16, origin: [f32; 3]) -> crate::event_entity::EventEntity {
    crate::map_turret_world::effect_event(
        crate::map_turret_world::EV_PLAY_EFFECT_ID,
        u32::from(effect),
        origin,
        [0.0, 0.0, 1.0],
    )
}

/// The wire fields an item's entity carries (protocol 26's entity field indices).
pub mod es {
    pub const POS_BASE: [usize; 3] = [2, 1, 4];
    pub const APOS_BASE: [usize; 3] = [5, 3, 33];
    pub const ANGLES: [usize; 3] = [25, 9, 24];
    pub const ORIGIN: [usize; 3] = [11, 12, 13];
    pub const TYPE: usize = 8;
    pub const ICON: usize = 18;
    pub const EFLAGS: usize = 19;
    pub const GROUND: usize = 22;
    pub const MODEL_INDEX: usize = 46;
    pub const MODEL_GHOUL2: usize = 54;
    pub const TIME2: usize = 61;
    pub const BOLT_TO_PLAYER: usize = 101;
}

/// The item's entity as a client is sent it (`ET_GENERAL`, standing where it is: its
/// `G_SetOrigin` trajectory, its angles, model, flags, radar icon and pulse, the player it
/// rides on, and the health bar of one that shows it).
pub fn project(item: &SiegeItem, state: &mut sjk_protocol::EntityState) {
    state.set_raw_field(es::TYPE, 0);
    for axis in 0..3 {
        state.set_raw_field(es::POS_BASE[axis], item.origin[axis].to_bits());
        state.set_raw_field(es::ORIGIN[axis], item.home[axis].to_bits());
        state.set_raw_field(es::APOS_BASE[axis], item.angles[axis].to_bits());
        state.set_raw_field(es::ANGLES[axis], item.angles[axis].to_bits());
    }
    state.set_raw_field(es::ICON, u32::from(item.icon));
    state.set_raw_field(es::EFLAGS, item.eflags);
    state.set_raw_field(es::GROUND, u32::from(item.ground));
    state.set_raw_field(es::MODEL_INDEX, u32::from(item.model_index));
    state.set_raw_field(es::MODEL_GHOUL2, u32::from(item.ghoul2));
    state.set_raw_field(es::TIME2, item.time2 as u32);
    state.set_raw_field(es::BOLT_TO_PLAYER, item.bolt_to_player);
    if item.max_health > 0 {
        crate::map_turret_world::publish_net_health(state, item.health, item.max_health);
    }
}
