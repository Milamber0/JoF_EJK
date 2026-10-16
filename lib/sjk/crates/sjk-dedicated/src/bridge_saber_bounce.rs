//! A blade bouncing off a wall on the native server (`SFL_BOUNCE_ON_WALLS`,
//! `w_saber.c:4453-4523`): the swinger's broken-parry pose, the bounce sound, the hit
//! event, and the splash (`WP_SaberRadiusDamage`).

use super::NativeGame;
use super::bridge_force::linked_box;
use sjk_game_jka::damage::{Attacker, DamageRequest};
use sjk_game_jka::event_entity::EventEntity;
use sjk_game_jka::means_of_death::MOD_MELEE;
use sjk_game_jka::saber_damage::{BounceSound, WallBounce};
use sjk_game_jka::saber_splash::{self, MAX_SPLASH_ENTITIES, SplashTarget, SplashWorld};

/// `EV_GENERAL_SOUND`, and the wire field its channel goes in (`saberEntityNum`).
const EV_GENERAL_SOUND: u32 = 76;
const ES_SABER_ENTITY: usize = 37;
/// `CHAN_AUTO`.
const CHAN_AUTO: u32 = 0;
/// `ENTITYNUM_NONE`: on no ground.
const ENTITY_NUMBER_NONE: u16 = 1_023;

impl NativeGame {
    /// `client`'s blade bounced off a wall at `level_time`.
    pub(super) fn saber_wall_bounce(
        &mut self,
        client: usize,
        bounce: &WallBounce,
        level_time: i32,
    ) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        if let Some(animation) = bounce.animation {
            use sjk_game_jka::pmove_anim::{
                SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE,
            };
            peer.movement.set_animation_parts(
                SETANIM_BOTH,
                animation,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            peer.movement.write_player_state(&mut peer.state);
        }
        let origin = peer.state.origin();
        // `WP_SaberBounceSound`: `G_Sound` on the swinger.
        let index = match bounce.sound {
            BounceSound::Registered(index) => index,
            BounceSound::Block(number) => {
                let name = format!("sound/weapons/saber/saberblock{number}.wav");
                let (sounds, told) = (&mut self.sounds, &mut self.told);
                sounds.index(name.as_bytes(), &mut |index, value| {
                    told.push(super::Told::ConfigString {
                        index,
                        previous: Vec::new(),
                        value: value.to_vec(),
                    })
                })
            }
        };
        let mut sound = EventEntity {
            event: EV_GENERAL_SOUND,
            parameter: u32::from(index),
            origin,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        sound.extra[0] = (ES_SABER_ENTITY, CHAN_AUTO);
        let _ = self.pool.spawn_temporary(sound.state(), level_time, None);
        let _ = self
            .pool
            .spawn_temporary(bounce.event.state(), level_time, None);
        let (radius, damage, knockback) = bounce.splash;
        saber_splash::radius_damage(
            client as u16,
            bounce.point,
            radius,
            damage,
            knockback,
            &mut Splash {
                game: self,
                swinger: client,
                level_time,
            },
        );
    }
}

/// The splash's reach on the server: the players, the NPCs and the breakable brushes.
struct Splash<'a> {
    game: &'a mut NativeGame,
    swinger: usize,
    level_time: i32,
}

impl SplashWorld for Splash<'_> {
    fn entities_in_box(
        &mut self,
        mins: [f32; 3],
        maxs: [f32; 3],
        out: &mut [u16; MAX_SPLASH_ENTITIES],
    ) -> usize {
        let overlaps = |(absmin, absmax): ([f32; 3], [f32; 3])| {
            (0..3).all(|axis| absmin[axis] <= maxs[axis] && absmax[axis] >= mins[axis])
        };
        let game = &*self.game;
        let players = (0..game.players.places())
            .filter(|client| {
                game.peer(*client)
                    .is_some_and(|peer| overlaps(linked_box(peer)))
            })
            .map(|client| client as u16);
        let npcs = game
            .npcs
            .roster
            .actors
            .iter()
            .filter(|npc| npc.begun() && overlaps(npc.link))
            .map(|npc| npc.number);
        let brushes = game
            .breakables
            .iter()
            .filter(|(_, brush)| {
                overlaps((
                    brush.bounds.0.map(|value| value - 1.0),
                    brush.bounds.1.map(|value| value + 1.0),
                ))
            })
            .map(|(number, _)| number.legacy_number());
        let mut count = 0;
        for (slot, number) in out.iter_mut().zip(players.chain(npcs).chain(brushes)) {
            *slot = number;
            count += 1;
        }
        out[..count].sort_unstable();
        count
    }

    fn target(&self, number: u16) -> Option<SplashTarget> {
        let game = &*self.game;
        if let Some(peer) = game.peer(usize::from(number)) {
            let playing = peer.begun && peer.body_active();
            return Some(SplashTarget {
                in_use: playing,
                client: true,
                origin: peer.state.origin(),
                health: peer.health,
                grounded: peer.state.ground_entity_num() != ENTITY_NUMBER_NONE,
                ..SplashTarget::default()
            });
        }
        if let Some(npc) = game
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == number)
        {
            return Some(sjk_game_jka::npc_saber_bounce::splash_target(npc));
        }
        let (_, brush) = game
            .breakables
            .iter()
            .find(|(ours, _)| ours.legacy_number() == number)?;
        Some(SplashTarget {
            in_use: true,
            breakable: brush.takes_damage,
            ..SplashTarget::default()
        })
    }

    fn hurt(&mut self, number: u16, damage: i32, flags: u32) {
        let (game, swinger, level_time) = (&mut *self.game, self.swinger, self.level_time);
        let npc = game
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == number)
            .map(|npc| npc.current_origin);
        if npc.is_none() && usize::from(number) >= game.players.places() {
            let _ = game.hurt_brush(number, damage, MOD_MELEE, swinger as u16, level_time);
            return;
        }
        let Some(me) = game.peer(swinger) else { return };
        let attacker = Attacker {
            npc: false,
            client: swinger as u16,
            max_health: me.state.max_health(),
            team: me.session.team,
            saber_knockback: [0.0; 4],
        };
        let point = game
            .peer(usize::from(number))
            .map(|peer| peer.state.origin())
            .or(npc);
        let request = DamageRequest {
            level_time,
            attacker: Some(attacker),
            direction: Some([0.0; 3]),
            point,
            damage,
            flags,
            means: MOD_MELEE,
        };
        let _ = game.strike_at(swinger, usize::from(number), request, None, false);
    }

    fn throw(&mut self, number: u16, direction: [f32; 3], push: f32) {
        // An NPC restarts its movement from its state at its next move.
        if let Some(npc) = self
            .game
            .npcs
            .roster
            .actors
            .iter_mut()
            .find(|npc| npc.number == number)
        {
            saber_splash::throw(&mut npc.player, direction, push);
            return;
        }
        if let Some(peer) = self.game.peer_mut(usize::from(number)) {
            saber_splash::throw(&mut peer.state, direction, push);
            peer.movement = peer.movement.reseeded(&peer.state);
        }
    }

    fn knock_down(&mut self, number: u16) {
        let level_time = self.level_time;
        if let Some(npc) = self
            .game
            .npcs
            .roster
            .actors
            .iter_mut()
            .find(|npc| npc.number == number)
        {
            sjk_game_jka::knockdown::knock_down(
                &mut npc.player,
                &mut npc.mind.knockdown,
                level_time,
            );
            return;
        }
        if let Some(peer) = self.game.peer_mut(usize::from(number)) {
            sjk_game_jka::knockdown::knock_down(&mut peer.state, &mut peer.knockdown, level_time);
            peer.movement = peer.movement.reseeded(&peer.state);
        }
    }
}
