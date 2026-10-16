//! Siege's half of a trigger's touch on the server (`g_trigger.c`'s siege gates and
//! hacks, `g_main.c`'s hacking checks): what the rules of
//! [`sjk_game_jka::siege_triggers`] read is gathered here before a touch, and what they
//! answered is done after it — a hack kept on the player, a carried item delivered.

use super::*;
use sjk_game_jka::siege_triggers::{self, Activator, Carried, Hack, Hacker};
use sjk_game_jka::triggers::{Multiple, TouchHooks};

/// `EF_DEAD`: a dead player holds no zone.
const EF_DEAD: u32 = 1 << 1;
/// `BUTTON_USE`.
const BUTTON_USE: u16 = 32;
/// `playerState_t`'s `hackingTime` and `hackingBaseTime` (protocol 26's player fields).
const PS_HACKING_TIME: usize = 91;
const PS_HACKING_BASE_TIME: usize = 124;
/// How long the kept console pose lasts each frame (`torsoTimer = 500`).
const POSE_HOLD: i32 = 500;

/// What a touch of one trigger reads of siege and of the player, and what it leaves.
pub(super) struct SiegeTouchHooks {
    siege: bool,
    round_begun: bool,
    /// The trigger as a hack names it: its place among the level's triggers, plus one.
    number: u16,
    absmin: [f32; 3],
    absmax: [f32; 3],
    class: Option<String>,
    team: i32,
    carried: Option<(Option<String>, i32, Option<String>)>,
    counts: (i32, i32),
    origin: [f32; 3],
    view_angles: [f32; 3],
    hack: Hack,
    level_time: i32,
    /// The carried item was delivered (with its `target3`).
    delivered: Option<Option<String>>,
}

impl TouchHooks for SiegeTouchHooks {
    fn hack(&mut self, trigger: &Multiple) -> bool {
        let who = Hacker {
            player: true,
            class: self.class.as_deref(),
            origin: self.origin,
            view_angles: self.view_angles,
        };
        siege_triggers::hack_gate(
            &trigger.siege,
            self.number,
            self.absmin,
            self.absmax,
            self.siege,
            &who,
            &mut self.hack,
            self.level_time,
        )
    }

    fn gate(&mut self, trigger: &mut Multiple) -> bool {
        let carried = self.carried.as_ref().map(|(goal, team, target3)| Carried {
            goaltarget: goal.as_deref(),
            team_no_complete: *team,
            target3: target3.as_deref(),
        });
        let activator = Activator {
            client: true,
            team: self.team,
            class: self.class.as_deref(),
            carried,
        };
        let counts = self.counts;
        let gate = siege_triggers::gates(
            &mut trigger.siege,
            trigger.allied_team,
            &trigger.targetname,
            self.siege,
            self.round_begun,
            Some(&activator),
            &mut || counts,
        );
        if gate.delivered.is_some() {
            self.delivered = gate.delivered;
        }
        gate.pass
    }
}

impl NativeGame {
    /// The trigger at `index`'s linked box (its model's box a unit bigger each way).
    fn trigger_abs_box(&self, index: usize) -> ([f32; 3], [f32; 3]) {
        let (low, high) = self.multiples[index].bounds;
        (low.map(|value| value - 1.0), high.map(|value| value + 1.0))
    }

    /// What player `client` touching the trigger at `index` brings to its siege rules.
    pub(super) fn siege_touch_hooks(
        &self,
        client: usize,
        index: usize,
        level_time: i32,
    ) -> SiegeTouchHooks {
        let (absmin, absmax) = self.trigger_abs_box(index);
        let siege = self.siege.as_ref();
        let peer = self.peer(client);
        let class = siege
            .zip(peer.and_then(|peer| peer.siege_class_index))
            .and_then(|(siege, class)| siege.registry.classes.get(class))
            .map(|class| class.name.clone());
        let counts = if self.multiples[index].siege.teambalance {
            self.zone_counts(absmin, absmax)
        } else {
            (0, 0)
        };
        SiegeTouchHooks {
            siege: self.gametype == GAMETYPE_SIEGE,
            round_begun: siege.is_some_and(|siege| siege.round.begun),
            number: index as u16 + 1,
            absmin,
            absmax,
            class,
            team: peer.map_or(0, |peer| peer.session.team),
            carried: self.carried_item(client),
            counts,
            origin: peer.map_or([0.0; 3], |peer| peer.state.origin()),
            view_angles: peer.map_or([0.0; 3], |peer| peer.state.view_angles()),
            hack: peer.map_or(Hack::default(), |peer| peer.siege_hands.hack),
            level_time,
            delivered: None,
        }
    }

    /// The living team 1 and team 2 players whose linked boxes meet `absmin`..`absmax`
    /// (`trap->EntitiesInBox`).
    fn zone_counts(&self, absmin: [f32; 3], absmax: [f32; 3]) -> (i32, i32) {
        let mut counts = (0, 0);
        for client in 0..self.players.places() {
            let Some(peer) = self.peer(client).filter(|peer| {
                peer.playing() && peer.health > 0 && peer.state.entity_flags() & EF_DEAD == 0
            }) else {
                continue;
            };
            let (origin, (mins, maxs)) = (peer.state.origin(), peer.movement.box_bounds());
            let inside = (0..3).all(|axis| {
                origin[axis] + mins[axis] - 1.0 <= absmax[axis]
                    && origin[axis] + maxs[axis] + 1.0 >= absmin[axis]
            });
            match peer.session.team {
                1 if inside => counts.0 += 1,
                2 if inside => counts.1 += 1,
                _ => {}
            }
        }
        counts
    }

    /// After a touch: the player's hack as it stands, and a delivered item gone.
    pub(super) fn siege_touch_done(
        &mut self,
        client: usize,
        hooks: SiegeTouchHooks,
        level_time: i32,
    ) {
        if let Some(peer) = self.peer_mut(client) {
            peer.siege_hands.hack = hooks.hack;
            publish_hack(&mut peer.state, &hooks.hack);
        }
        if let Some(target3) = hooks.delivered {
            self.deliver_siege_item(client, target3, level_time);
        }
    }

    /// `G_RunFrame`'s hacking checks for every hacker (`g_main.c:3206-3244`): the console
    /// pose kept, and the hack ended where the rules end it.
    pub(super) fn run_siege_hacks(&mut self) {
        for client in 0..self.players.places() {
            let Some(hack) = self
                .peer(client)
                .map(|peer| peer.siege_hands.hack)
                .filter(|hack| hack.trigger != 0)
            else {
                continue;
            };
            let index = usize::from(hack.trigger) - 1;
            let hacked = (index < self.multiples.len()).then(|| self.trigger_abs_box(index));
            let Some(peer) = self.peer_mut(client) else {
                continue;
            };
            if peer.state.raw_field(15).unwrap_or(0) as u16 != sjk_game_jka::triggers::BOTH_CONSOLE1
            {
                peer.movement.set_animation_parts(
                    sjk_game_jka::pmove_anim::SETANIM_TORSO,
                    sjk_game_jka::triggers::BOTH_CONSOLE1,
                    sjk_game_jka::pmove_anim::SETANIM_FLAG_OVERRIDE
                        | sjk_game_jka::pmove_anim::SETANIM_FLAG_HOLD,
                );
            } else {
                peer.movement.set_torso_timer(POSE_HOLD);
            }
            let timer = peer.movement.state().torso_timer;
            peer.movement.set_weapon_time(timer);
            peer.movement.write_player_state(&mut peer.state);
            let using = peer.last_command.buttons & BUTTON_USE != 0;
            let mut hack = peer.siege_hands.hack;
            siege_triggers::hack_frame(
                &mut hack,
                using,
                hacked,
                peer.state.origin(),
                peer.state.view_angles(),
            );
            peer.siege_hands.hack = hack;
            publish_hack(&mut peer.state, &hack);
        }
    }

    /// `ClientSpawn`'s clearing of the client (`memset` of `gclient_t` and its player
    /// state): no item held, no hack, no hacking bar.
    pub(super) fn siege_client_spawned(&mut self, client: usize) {
        if let Some(peer) = self.peer_mut(client) {
            peer.siege_hands = Default::default();
            publish_hack(&mut peer.state, &Hack::default());
        }
    }

    /// `player_die`'s `isHacking = 0; ps.hackingTime = 0`.
    pub(super) fn siege_hack_ends(&mut self, client: usize) {
        if let Some(peer) = self.peer_mut(client) {
            peer.siege_hands.hack.trigger = 0;
            peer.siege_hands.hack.time = 0;
            let hack = peer.siege_hands.hack;
            publish_hack(&mut peer.state, &hack);
        }
    }
}

/// A hack's time and length onto the player's state, where its client draws the bar from.
fn publish_hack(state: &mut PlayerState, hack: &Hack) {
    state.set_raw_field(PS_HACKING_TIME, hack.time as u32);
    state.set_raw_field(PS_HACKING_BASE_TIME, hack.base_time as u32);
}
