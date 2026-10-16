//! The bots' frame on this server (`SV_BotFrame` → `BotAIStartFrame`): each bot thinks
//! (`BotAI`: `StandardBotAI`'s opening, then its body against the server's views) and
//! what its frame does to the game is carried out; its command is kept as a player's is
//! (`BotUserCommand` → `SV_ClientThink`); the game frame then runs it at the level's time
//! (`G_RunClient`). See [`sjk_game_jka::bot_think`] and [`sjk_game_jka::bot_standard`].
//!
//! The bots of one frame see the world as it was before any of them thought; the
//! reference's later bots see what an earlier one's saber switch or duel answer did.

use super::bridge_bot_views::{BotTraces, ServerMoves, ServerSenses};
use super::*;
use sjk_game_jka::bot_ctf::CtfGame;
use sjk_game_jka::bot_objectives::ObjectiveGame;
use sjk_game_jka::bot_senses::SensingRules;
use sjk_game_jka::bot_squad::SquadGame;
use sjk_game_jka::bot_standard::{BotActs, StandardGame, standard_bot_ai};
use sjk_game_jka::bot_tactics::damage_notification;
use sjk_game_jka::bot_think::{BotVariables, BotView};
use sjk_game_jka::bot_trail::TrailGame;
use sjk_game_jka::force_powers::{UsableOn, usable_on};
use sjk_game_jka::generic_commands::{
    GENCMD_ENGAGE_DUEL, GENCMD_SABERATTACKCYCLE, GENCMD_SABERSWITCH,
};
use std::cell::Cell;

/// `fd.forcePowerSelected` on the wire.
const PS_FORCE_POWER_SELECTED: usize = 54;
/// `PMF_JUMP_HELD`.
const PMF_JUMP_HELD: u16 = 2;
/// `STAT_HOLDABLE_ITEM`.
const STAT_HOLDABLE_ITEM: usize = 1;
/// `PDSOUND_ABSORBHIT`, and the field its victim goes in (`trickedentindex`).
const PDSOUND_ABSORBHIT: u32 = 3;
const ES_TRICKED: usize = 58;

impl NativeGame {
    /// `BotAIStartFrame` at `time`, after the spawn queue (`G_CheckBotSpawn`), with the
    /// last game frame's time as `level.time`. Nothing without `bot_enable`.
    pub(crate) fn bot_frame(&mut self, time: i32) {
        if !self.bots_enabled() {
            return;
        }
        let level_time = self.last_frame_time;
        let forgimmick = self.cvars.integer(b"bot_forgimmick");
        let frame = self
            .bots
            .clock
            .start_frame(time, level_time, || BotVariables { forgimmick });
        let variables = self.bots.clock.variables;
        self.forget_departed_bots();
        self.gather_bot_views();
        self.update_bot_trackers(level_time);
        let mut siege_things = std::mem::take(&mut self.bots.views.siege_things);
        self.siege_things(&mut siege_things);
        self.bots.views.siege_things = siege_things;
        // Client 0's origin, for the mode that walks to it (`g_entities[0]` in use).
        let first_client = self
            .peer(0)
            .filter(|peer| peer.begun)
            .map(|peer| peer.state.origin());
        // Every bot thinks, in slot order; a bot not in the game yet only counts the time.
        for client in 0..self.players.places() {
            let Some(peer) = self.peer(client) else {
                continue;
            };
            let view = BotView {
                origin: peer.state.origin(),
                viewheight: peer.state.view_height(),
                delta_angles: peer.state.delta_angles(),
                team: peer.session.team,
                first_client,
            };
            let begun = peer.begun;
            let Some(mind) = self.bots.minds.get_mut(client).and_then(Option::as_mut) else {
                continue;
            };
            if !(mind.due(&frame) && begun) {
                continue;
            }
            if mind.begin_think(&view, &frame, &variables) {
                self.think_body(client, level_time);
            }
            if let Some(mind) = self.bots.minds[client].as_mut() {
                mind.end_think();
            }
        }
        // Then every bot in the game makes its command, which the server keeps as the
        // one it last heard from the bot.
        let Self {
            server,
            world,
            players,
            deaths,
            bots,
            ..
        } = self;
        let Some(world) = server.world_mut(*world) else {
            return;
        };
        for (client, handle) in players.holders().enumerate() {
            let Some(peer) = handle.and_then(|handle| world.entity_mut(handle)) else {
                continue;
            };
            if !peer.begun {
                continue;
            }
            let Some(mind) = bots.minds.get_mut(client).and_then(Option::as_mut) else {
                continue;
            };
            let command = mind.update_input(&frame, level_time, &mut deaths.rng);
            peer.last_command = command;
            peer.last_command_time = level_time;
        }
    }

    /// `G_Damage`'s blow from client `attacker` landed on player `target`: the bots are
    /// told (`BotDamageNotification`), then the flag game (`Team_CheckHurtCarrier`).
    pub(crate) fn blow_landed(&mut self, attacker: usize, target: usize, level_time: i32) {
        if self.bots.minds.iter().any(Option::is_some) {
            self.gather_bot_views();
            let rules = SensingRules {
                level_time,
                gametype: self.gametype,
                level_flags: self.bots.routes.level_flags,
                friendly_fire: self.cvars.integer(b"g_friendlyFire") != 0,
                attachments: self.cvars.integer(b"bot_attachments") != 0,
            };
            let Self { map, bots, .. } = self;
            let Bots { minds, views, .. } = bots;
            let senses = ServerSenses {
                traces: BotTraces {
                    map: map.as_ref(),
                    obstacles: &[],
                },
                views,
            };
            damage_notification(minds, target, attacker, &senses, &rules);
        }
        self.flag_blow(attacker, target, level_time);
    }

    /// A slot whose bot left holds no mind (`BotAIShutdownClient`).
    fn forget_departed_bots(&mut self) {
        let count = self.players.places();
        self.bots.minds.resize(count, None);
        for client in 0..count {
            if !self.peer(client).is_some_and(|peer| peer.bot) {
                self.bots.minds[client] = None;
            }
        }
    }

    /// `StandardBotAI`'s body for bot `client` against the server's views, and what it
    /// did to the game carried out.
    fn think_body(&mut self, client: usize, level_time: i32) {
        self.gather_obstacles(client);
        let (mut ammo, mut powerups) = ([0; 16], [0; 16]);
        let Some(me) = self.bot_self(client, &mut ammo, &mut powerups) else {
            return;
        };
        let Some(peer) = self.peer(client) else {
            return;
        };
        let weights = peer
            .personality
            .as_ref()
            .map(|personality| personality.weapon_weights)
            .unwrap_or_default();
        let origin = peer.state.origin();
        let integer = |name: &[u8]| self.cvars.integer(name);
        let (friendly_fire, attachments, camp) = (
            integer(b"g_friendlyFire") != 0,
            integer(b"bot_attachments") != 0,
            integer(b"bot_camp") != 0,
        );
        let (english, honorable_duels, private_duels) = (
            integer(b"se_language") == 0,
            integer(b"bot_honorableduelacceptance") != 0,
            integer(b"g_privateDuel") != 0,
        );
        let force_powers = integer(b"bot_forcepowers") != 0 && integer(b"g_forcePowerDisable") == 0;
        let (dropped, jedi_master_saber) = (self.dropped_flags(), self.jedi_master_saber());
        // `imperial_attackers`, `rebel_attackers`: the map's sides in the order it names them.
        let attacking = |side: usize| self.siege_attackers(side);
        let attackers = (attacking(0), attacking(1));
        let absorbed = Cell::new(false);
        let enemy = self.bots.minds[client]
            .as_ref()
            .and_then(|mind| mind.current_enemy);
        let acts = {
            let Self {
                server,
                world,
                players,
                map,
                obstacles,
                items,
                bots,
                deaths,
                rand,
                gametype,
                ..
            } = self;
            let gametype = *gametype;
            let world = server.world(*world);
            let state_of = |number: Option<i32>| {
                let handle = players.at(usize::try_from(number?).ok()?)?;
                world?.entity(handle).map(|peer| &peer.state)
            };
            let own_state = state_of(Some(client as i32));
            // `ForcePowerUsableOn(bot, currentEnemy, power)`: the enemy it had as it chose.
            let enemy_state = state_of(
                bots.minds[client]
                    .as_ref()
                    .and_then(|mind| mind.current_enemy),
            );
            let usable_on_enemy = |power: usize| match (own_state, enemy_state) {
                (Some(own), Some(enemy)) => {
                    match usable_on(own, enemy, power, level_time, gametype) {
                        UsableOn::Yes => true,
                        UsableOn::No => false,
                        UsableOn::Absorbed => {
                            absorbed.set(true);
                            false
                        }
                    }
                }
                _ => false,
            };
            let item_of = |entity: i32| {
                items
                    .iter()
                    .find(|(number, _)| i32::from(number.legacy_number()) == entity)
                    .map(|(_, pickup)| &sjk_game_jka::items::ITEMS[pickup.item])
            };
            let Bots {
                minds,
                routes,
                views,
                ..
            } = bots;
            let level_flags = routes.level_flags;
            let rules = SensingRules {
                level_time,
                gametype,
                level_flags,
                friendly_fire,
                attachments,
            };
            let game = StandardGame {
                me,
                rules,
                trackers: &views.trackers,
                trail: TrailGame {
                    level_time,
                    gametype,
                    level_flags,
                    camp,
                },
                ctf: CtfGame {
                    gametype,
                    level_time,
                    clients: &views.ctf,
                    dropped,
                },
                objectives: ObjectiveGame {
                    gametype,
                    level_time,
                    clients: &views.objective,
                    things: &views.siege_things,
                    attackers,
                    jedi_master_saber,
                },
                squad: SquadGame {
                    gametype,
                    level_time,
                    attachments,
                    clients: &views.squad,
                },
                english,
                honorable_duels,
                private_duels,
                force_powers,
                weights: &weights,
                item_of: &item_of,
                usable_on_enemy: &usable_on_enemy,
            };
            let traces = || BotTraces {
                map: map.as_ref(),
                obstacles,
            };
            let mut senses = ServerSenses {
                traces: traces(),
                views,
            };
            let mut moves = ServerMoves {
                traces: traces(),
                views,
                gametype,
            };
            standard_bot_ai(
                minds,
                client,
                &game,
                routes,
                &mut senses,
                &mut moves,
                &mut deaths.rng,
                rand,
            )
        };
        self.carry_out_bot_acts(client, &acts, origin, level_time);
        if absorbed.get()
            && let Some(enemy) = enemy.and_then(|enemy| usize::try_from(enemy).ok())
        {
            self.absorbed_grip(enemy, level_time);
        }
    }

    /// `ForcePowerUsableOn`'s refusal of a grip at a player absorbing: the absorbing
    /// hit's sound, at most every 400 ms.
    fn absorbed_grip(&mut self, victim: usize, level_time: i32) {
        let Some(peer) = self.peer_mut(victim) else {
            return;
        };
        if peer.force.sound_debounce >= level_time {
            return;
        }
        peer.force.sound_debounce = level_time + 400;
        let mut event =
            sjk_game_jka::knockdown::predef_sound(peer.state.origin(), PDSOUND_ABSORBHIT);
        event.extra[3] = (ES_TRICKED, victim as u32);
        let _ = self.pool.spawn_temporary(event.state(), level_time, None);
    }

    /// What a bot's frame did to the game, as the reference does it at once: the Force
    /// power selected, the saber switched, the duel answered, the style cycled (the keys'
    /// own commands), the jump held, the item taken in hand, the line said.
    fn carry_out_bot_acts(
        &mut self,
        client: usize,
        acts: &BotActs,
        origin: [f32; 3],
        level_time: i32,
    ) {
        if let Some(peer) = self.peer_mut(client) {
            let mut changed = false;
            if let Some(power) = acts.force_selected {
                changed |= peer
                    .state
                    .set_raw_field(PS_FORCE_POWER_SELECTED, power as u32);
            }
            if acts.jump_held {
                peer.state
                    .set_movement_flags(peer.state.movement_flags() | PMF_JUMP_HELD);
                changed = true;
            }
            if let Some(tag) = acts.holdable {
                let items = &sjk_game_jka::items::ITEMS;
                if let Some(index) = items.iter().position(|item| {
                    item.kind == sjk_game_jka::items::Kind::Holdable && item.tag == tag
                }) {
                    peer.state.stats[STAT_HOLDABLE_ITEM] = index as u32;
                    changed = true;
                }
            }
            if changed {
                peer.movement = peer.movement.reseeded(&peer.state);
            }
        }
        for (wanted, generic) in [
            (acts.toggle_saber, GENCMD_SABERSWITCH),
            (acts.engage_duel, GENCMD_ENGAGE_DUEL),
            (acts.cycle_saber_style, GENCMD_SABERATTACKCYCLE),
        ] {
            if wanted {
                self.generic_command(
                    client,
                    &UserCommand {
                        server_time: level_time,
                        generic_command: generic,
                        ..UserCommand::default()
                    },
                    origin,
                    level_time,
                );
            }
        }
        if let Some((line, team)) = &acts.said {
            let mut command = if *team {
                b"say_team ".to_vec()
            } else {
                b"say ".to_vec()
            };
            command.extend_from_slice(line);
            self.player_command(client, &command, level_time);
        }
    }

    /// A new level: every bot's thinking starts over (`BotAILoadMap`'s `BotResetState`
    /// on a restart, `BotAISetupClient` on a new map), and on a new map the game module's
    /// clock with it.
    pub(crate) fn reset_bot_minds(&mut self, new_map: bool) {
        if new_map {
            self.bots.clock = Default::default();
        }
        for mind in self.bots.minds.iter_mut().flatten() {
            mind.reset();
        }
    }

    /// `G_RunClient` for a bot: its last command run at the level's time
    /// (`ClientThink_real`); a player's own commands run as they arrive.
    pub(crate) fn run_bot_client(&mut self, client: usize, server_time: i32) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        if !peer.bot || !peer.begun {
            return;
        }
        let command = UserCommand {
            server_time,
            ..peer.last_command
        };
        self.player_think(client, &command, server_time);
    }
}
