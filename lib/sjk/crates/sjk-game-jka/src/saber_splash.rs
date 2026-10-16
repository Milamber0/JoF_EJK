//! A saber's splash where it bounces off a wall: `WP_SaberRadiusDamage` (OpenJK
//! `codemp/game/w_saber.c:3680-3769`) and the `G_Throw` it knocks players back with
//! (`g_utils.c:336-390`).

use crate::saber_clash::normalize;

/// `g_knockback`'s default.
const KNOCKBACK: f64 = 1_000.0;
/// `PMF_TIME_KNOCKBACK`.
const PMF_TIME_KNOCKBACK: u16 = 0x40;
/// The most entities `trap->EntitiesInBox` answers with here.
pub const MAX_SPLASH_ENTITIES: usize = 128;

/// An entity within the splash's box, as the splash reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SplashTarget {
    /// `inuse`.
    pub in_use: bool,
    /// A client (a player or an NPC).
    pub client: bool,
    /// `G_EntIsBreakable`, for what is not a client.
    pub breakable: bool,
    /// `EF2_HELD_BY_MONSTER`.
    pub held_by_monster: bool,
    /// A rancor or an AT-ST, or `FL_NO_KNOCKBACK`: not thrown back.
    pub unthrowable: bool,
    /// `r.currentOrigin`.
    pub origin: [f32; 3],
    /// `health`, read again after the damage.
    pub health: i32,
    /// `ps.groundEntityNum != ENTITYNUM_NONE`.
    pub grounded: bool,
}

/// What the splash reaches and does.
pub trait SplashWorld {
    /// `trap->EntitiesInBox`: the entities whose boxes touch, into `out`; how many.
    fn entities_in_box(
        &mut self,
        mins: [f32; 3],
        maxs: [f32; 3],
        out: &mut [u16; MAX_SPLASH_ENTITIES],
    ) -> usize;
    /// Entity `number`, where it is one.
    fn target(&self, number: u16) -> Option<SplashTarget>;
    /// `G_Damage(target, ent, ent, vec3_origin, target->r.currentOrigin, damage, flags,
    /// MOD_MELEE)`.
    fn hurt(&mut self, number: u16, damage: i32, flags: u32);
    /// `G_Throw` on a client ([`throw`]).
    fn throw(&mut self, number: u16, direction: [f32; 3], push: f32);
    /// `G_Knockdown`.
    fn knock_down(&mut self, number: u16);
}

/// `WP_SaberRadiusDamage(ent, point, radius, damage, knockBack)` by `swinger`.
pub fn radius_damage(
    swinger: u16,
    point: [f32; 3],
    radius: f32,
    damage: i32,
    knockback: f32,
    world: &mut dyn SplashWorld,
) {
    if radius <= 0.0 || (damage <= 0 && knockback <= 0.0) {
        return;
    }
    let mins = point.map(|value| value - radius);
    let maxs = point.map(|value| value + radius);
    let mut numbers = [0_u16; MAX_SPLASH_ENTITIES];
    let count = world
        .entities_in_box(mins, maxs, &mut numbers)
        .min(MAX_SPLASH_ENTITIES);
    for &number in &numbers[..count] {
        let Some(target) = world.target(number) else {
            continue;
        };
        if !target.in_use || number == swinger {
            continue;
        }
        if !target.client {
            // "damage breakables within range, but not as much"
            if target.breakable {
                world.hurt(number, 10, 0);
            }
            continue;
        }
        if target.held_by_monster {
            continue;
        }
        let mut direction = std::array::from_fn(|axis| target.origin[axis] - point[axis]);
        let distance = normalize(&mut direction);
        if distance > radius {
            continue;
        }
        if damage > 0 {
            let points = (f64::from(damage as f32 * distance / radius)).ceil() as i32;
            world.hurt(number, points, crate::damage::DAMAGE_NO_KNOCKBACK);
        }
        if knockback > 0.0 && !target.unthrowable {
            let strength = knockback * distance / radius;
            direction[2] += 0.1;
            normalize(&mut direction);
            world.throw(number, direction, strength);
            let alive = world.target(number).is_some_and(|target| target.health > 0);
            // "close enough and knockback high enough to possibly knock down"
            if alive && strength > 50.0 && (distance < radius * 0.5 || target.grounded) {
                world.knock_down(number);
            }
        }
    }
}

/// `G_Throw` on a client (mass 200, gravity on): 0.8 of the knockback across and 1.5 of
/// it up added to its velocity, and — where it has no `pm_time` — twice the push, 50 to
/// 200 ms, as `PMF_TIME_KNOCKBACK`, "so that the other client can't cancel out the
/// movement immediately".
pub fn throw(state: &mut sjk_protocol::PlayerState, direction: [f32; 3], push: f32) {
    const KNOCKBACK_F32: f32 = KNOCKBACK as f32;
    // `g_knockback.value * (float)push / mass * 0.8`: a float product, then a double one.
    let across = (f64::from(KNOCKBACK_F32 * push / 200.0) * 0.8) as f32;
    let up = (f64::from(direction[2] * KNOCKBACK_F32 * push / 200.0) * 1.5) as f32;
    let velocity = state.velocity();
    state.set_velocity([
        velocity[0] + direction[0] * across,
        velocity[1] + direction[1] * across,
        velocity[2] + up,
    ]);
    if state.movement_time() == 0 {
        state.set_movement_time(((push * 2.0) as i32).clamp(50, 200) as i16);
        state.set_movement_flags(state.movement_flags() | PMF_TIME_KNOCKBACK);
    }
}
