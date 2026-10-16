//! The commands a client's keys send in a usercmd's `generic_cmd` (`ClientThink_real`,
//! `g_active.c:3109-3330`), after the move: the saber switch, the style cycle, the Force
//! powers' keys and the taunts ([`sjk_game_jka::generic_commands`]).

use super::NativeGame;
use sjk_game_jka::event_entity::EventEntity;
use sjk_game_jka::force_powers::{
    FP_ABSORB, FP_HEAL, FP_PROTECT, FP_PULL, FP_PUSH, FP_RAGE, FP_SEE, FP_SPEED, FP_TEAM_FORCE,
    FP_TEAM_HEAL, FP_TELEPATHY,
};
use sjk_game_jka::generic_commands::*;
use sjk_protocol::UserCommand;

/// `EV_GENERAL_SOUND`, and the wire field its channel goes in (`saberEntityNum`).
const EV_GENERAL_SOUND: u32 = 76;
const ES_SABER_ENTITY: usize = 37;
/// `FP_SABER_OFFENSE`: the styles a player may cycle through.
const FP_SABER_OFFENSE: usize = 15;
/// `CHAN_AUTO` for the switch, `CHAN_WEAPON` for the taunts.
const CHAN_AUTO: u32 = 0;
const CHAN_WEAPON: u32 = 2;

impl NativeGame {
    /// `client`'s generic command, taken this think (see [`take`]). `origin` is where it
    /// stood before the move (`r.currentOrigin`), where its saber's sounds are.
    pub(super) fn generic_command(
        &mut self,
        client: usize,
        command: &UserCommand,
        origin: [f32; 3],
        level_time: i32,
    ) {
        let generic = command.generic_command;
        let power = match generic {
            GENCMD_FORCE_HEAL => Some(FP_HEAL),
            GENCMD_FORCE_SPEED => Some(FP_SPEED),
            GENCMD_FORCE_THROW => Some(FP_PUSH),
            GENCMD_FORCE_PULL => Some(FP_PULL),
            GENCMD_FORCE_DISTRACT => Some(FP_TELEPATHY),
            GENCMD_FORCE_HEALOTHER => Some(FP_TEAM_HEAL),
            GENCMD_FORCE_FORCEPOWEROTHER => Some(FP_TEAM_FORCE),
            GENCMD_FORCE_RAGE => Some(FP_RAGE),
            GENCMD_FORCE_PROTECT => Some(FP_PROTECT),
            GENCMD_FORCE_ABSORB => Some(FP_ABSORB),
            GENCMD_FORCE_SEEING => Some(FP_SEE),
            _ => None,
        };
        if let Some(power) = power {
            let throw = self
                .with_force_frame(client, level_time, |state, force, frame| {
                    sjk_game_jka::force_powers::key(state, force, power, frame)
                })
                .flatten();
            // The key changed the pool and the powers on: the movement restarts from them.
            if let Some(peer) = self.peer_mut(client) {
                peer.movement = peer.movement.reseeded(&peer.state);
            }
            if let Some(pull) = throw {
                self.force_throw(client, pull, level_time);
            }
            return;
        }
        if generic == GENCMD_ENGAGE_DUEL {
            self.duel_key(client, level_time);
            return;
        }
        if self.generic_holdable(client, generic, level_time) {
            return;
        }
        let gametype = self.gametype;
        let lengths = self.map.as_ref().and_then(|map| map.animations.clone());
        let empty = sjk_game_jka::AnimationLengthTable::new([]);
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let mut sounds = Vec::new();
        let mut channel = CHAN_AUTO;
        let event_before = peer.state.raw_field(56);
        match generic {
            GENCMD_SABERSWITCH => {
                if toggle_saber(&mut peer.state, level_time, &mut sounds) {
                    self.knock_down_in_flight(client, level_time);
                    return;
                }
            }
            GENCMD_SABERATTACKCYCLE => {
                let mut base = peer.movement.state().saber_anim_level_base;
                let offense = peer
                    .session
                    .force
                    .as_ref()
                    .map_or(0, |force| force.levels[FP_SABER_OFFENSE]);
                sjk_game_jka::saber_stance::attack_cycle(
                    &mut peer.state,
                    &peer.sabers.hands,
                    offense,
                    &mut peer.saber.cycle_queue,
                    &mut base,
                    &mut sounds,
                );
                peer.movement.set_saber_anim_level_base(base);
            }
            GENCMD_TAUNT..=GENCMD_GLOAT => {
                channel = CHAN_WEAPON;
                let lengths_ref: &dyn sjk_game_jka::AnimationLengths =
                    lengths.as_deref().map_or(&empty, |lengths| lengths);
                taunt(
                    &mut peer.state,
                    &peer.sabers.hands,
                    command,
                    u32::from(generic - GENCMD_TAUNT),
                    gametype,
                    level_time,
                    lengths_ref,
                    &mut peer.knockdown.hand_extend_time,
                    &mut sounds,
                );
            }
            _ => return,
        }
        // `G_AddEvent`'s `eventTime`: a taunt's `EV_TAUNT` lives its 300 ms.
        if peer.state.raw_field(56) != event_before {
            peer.entity.event_raised(level_time);
        }
        peer.movement = peer.movement.reseeded(&peer.state);
        // Each saber's own sounds (`saber[n].soundOn`, `soundOff`).
        let Some(hands) = self.peer(client).map(|peer| {
            peer.sabers
                .hands
                .each_ref()
                .map(|saber| (saber.sound_on, saber.sound_off, saber.is_held()))
        }) else {
            return;
        };
        for played in sounds.into_iter().filter(|sound| sound.plays(hands[1].2)) {
            let (on, off, _) = hands[usize::from(played.hand)];
            let index = if played.on { on } else { off };
            if index == 0 {
                continue;
            }
            let mut sound = EventEntity {
                event: EV_GENERAL_SOUND,
                parameter: u32::from(index),
                origin,
                client: None,
                broadcast: false,
                extra: [(0, 0); 12],
            };
            sound.extra[0] = (ES_SABER_ENTITY, channel);
            let _ = self.pool.spawn_temporary(sound.state(), level_time, None);
        }
    }
}
