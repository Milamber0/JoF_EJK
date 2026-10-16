//! An NPC's blade bouncing off a wall (`SFL_BOUNCE_ON_WALLS`, `w_saber.c:4453-4523`): the
//! swinger's broken-parry pose, `WP_SaberBounceSound`, the `EV_SABER_HIT` naming nobody,
//! and `WP_SaberRadiusDamage` (`w_saber.c:3680-3769`) from where it struck — on the
//! players and the NPCs through the creatures' reach ([`crate::npc_creature`]), on the
//! breakables through the host. No stock saber has the flag; installed custom ones do.

use crate::npc_creature::{CHAN_AUTO, EF2_HELD_BY_MONSTER};
use crate::npc_spawn::{NpcActor, NpcHost};
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
use crate::saber_damage::{BounceSound, WallBounce};
use crate::saber_splash::{MAX_SPLASH_ENTITIES, SplashTarget, SplashWorld};

/// `MOD_MELEE`.
const MOD_MELEE: u32 = 1;
/// `CLASS_ATST`, `CLASS_RANCOR`: never thrown back by a splash.
const CLASS_ATST: i32 = 1;
const CLASS_RANCOR: i32 = 54;
/// `FL_NO_KNOCKBACK`.
const FL_NO_KNOCKBACK: u32 = 0x800;

impl<H: NpcHost> NpcWorld<'_, H> {
    /// The blade of the NPC at `me` bounced off a wall: `bounce` carried out in the
    /// reference's order.
    pub(crate) fn npc_saber_wall_bounce(&mut self, me: usize, bounce: &WallBounce) {
        if let Some(animation) = bounce.animation {
            self.set_animation(
                me,
                SETANIM_BOTH,
                animation,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
        }
        // `WP_SaberBounceSound`: `G_Sound(ent, CHAN_AUTO, index)` on the swinger.
        let index = match bounce.sound {
            BounceSound::Registered(index) => index,
            BounceSound::Block(number) => self
                .host
                .sound_index(format!("sound/weapons/saber/saberblock{number}.wav").as_bytes()),
        };
        let origin = self.actors[me].current_origin;
        self.host
            .raise(crate::weapon_fire::sound_event(origin, CHAN_AUTO, index));
        self.host.raise(bounce.event);
        let (radius, damage, knockback) = bounce.splash;
        let swinger = self.actors[me].number;
        crate::saber_splash::radius_damage(
            swinger,
            bounce.point,
            radius,
            damage,
            knockback,
            &mut Splash {
                world: self,
                me,
                found: Vec::new(),
            },
        );
    }
}

/// NPC `npc` as a saber's wall-bounce splash reads it (`WP_SaberRadiusDamage`): a client,
/// spared while a monster holds it, never thrown when a rancor, an AT-ST or
/// `FL_NO_KNOCKBACK`.
pub fn splash_target(npc: &NpcActor) -> SplashTarget {
    let class = npc.definition.client_class;
    SplashTarget {
        in_use: true,
        client: true,
        held_by_monster: npc
            .player
            .raw_field(crate::npc_creature::PS_EFLAGS2)
            .unwrap_or(0)
            & EF2_HELD_BY_MONSTER
            != 0,
        unthrowable: class == CLASS_RANCOR
            || class == CLASS_ATST
            || npc.flags & FL_NO_KNOCKBACK != 0,
        origin: npc.current_origin,
        health: npc.health,
        grounded: npc.player.ground_entity_num() != crate::npc_spawn::ENTITYNUM_NONE,
        ..SplashTarget::default()
    }
}

/// The splash's reach from an NPC swinger: the clients (players and NPCs) and the host's
/// breakables.
struct Splash<'w, 'a, H: NpcHost> {
    world: &'w mut NpcWorld<'a, H>,
    me: usize,
    /// Scratch for the entities found (a bounce is rare; not a per-frame cost).
    found: Vec<u16>,
}

impl<H: NpcHost> SplashWorld for Splash<'_, '_, H> {
    fn entities_in_box(
        &mut self,
        mins: [f32; 3],
        maxs: [f32; 3],
        out: &mut [u16; MAX_SPLASH_ENTITIES],
    ) -> usize {
        let meets = |absmin: [f32; 3], absmax: [f32; 3]| {
            (0..3).all(|axis| absmin[axis] <= maxs[axis] && absmax[axis] >= mins[axis])
        };
        self.found.clear();
        for player in self.world.host.players() {
            let absmin = std::array::from_fn(|axis| player.origin[axis] + player.mins[axis] - 1.0);
            let absmax = std::array::from_fn(|axis| player.origin[axis] + player.maxs[axis] + 1.0);
            if meets(absmin, absmax) {
                self.found.push(player.number);
            }
        }
        for &at in self.world.order {
            let npc = &self.world.actors[at];
            if npc.begun() && meets(npc.link.0, npc.link.1) {
                self.found.push(npc.number);
            }
        }
        self.world
            .host
            .breakables_in_box(mins, maxs, &mut self.found);
        self.found.sort_unstable();
        let count = self.found.len().min(MAX_SPLASH_ENTITIES);
        out[..count].copy_from_slice(&self.found[..count]);
        count
    }

    fn target(&self, number: u16) -> Option<SplashTarget> {
        let world = &*self.world;
        if let Some(at) = world.actor_at(number) {
            return Some(splash_target(&world.actors[at]));
        }
        if let Some(player) = world
            .host
            .players()
            .iter()
            .find(|player| player.number == number)
        {
            let (held, grounded) = world
                .host
                .player_splash_state(number)
                .unwrap_or((false, false));
            return Some(SplashTarget {
                in_use: true,
                client: true,
                held_by_monster: held,
                origin: player.origin,
                health: player.health,
                grounded,
                ..SplashTarget::default()
            });
        }
        world
            .host
            .breakable_takes_damage(number)
            .map(|breakable| SplashTarget {
                in_use: true,
                breakable,
                ..SplashTarget::default()
            })
    }

    fn hurt(&mut self, number: u16, damage: i32, flags: u32) {
        let world = &mut *self.world;
        if world.actor_at(number).is_none()
            && !world
                .host
                .players()
                .iter()
                .any(|player| player.number == number)
        {
            let swinger = world.actors[self.me].number;
            world.host.hurt_breakable(number, damage, swinger);
            return;
        }
        let point = world.body(number).map(|body| body.origin);
        let direction = Some([0.0; 3]);
        world.creature_damage(self.me, number, direction, point, damage, flags, MOD_MELEE);
    }

    fn throw(&mut self, number: u16, direction: [f32; 3], push: f32) {
        self.world.creature_throw(number, direction, push);
    }

    fn knock_down(&mut self, number: u16) {
        self.world.creature_knockdown(number);
    }
}
