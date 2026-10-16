//! A playing client's grip, lightning and drain over everyone else on the server
//! ([`NativeGame::with_force_frame`], `WP_ForcePowersUpdate`'s dark powers,
//! `w_force.c:1566-2440`), with the NPCs' side of `G_Damage` dealt inline.
//!
//! The reference's power calls `G_Damage` on each target as it reaches it, and an NPC's
//! pain or death (`NPC_Pain`, `player_die`) is over — its draws from the generator, its
//! body's contents, the names it fires — before the power goes on to the next target.
//! So the roster is taken out of the game for the update and the NPCs' host is built
//! around it ([`ServerHost`]): the power reaches the players, the pool, the sounds and
//! the generator through the host, and a blow on an NPC runs through the roster at once.
//! A blow on another player still waits for the update to be over: the player's side of
//! `G_Damage` reaches the whole game (its death, the scores, the log), which the update
//! holds apart.

use super::super::bridge_force::{linked_box, remember_touched};
use super::super::*;
use super::ServerHost;
use crate::collision::{Void, WithPlayers, WorldCollision};
use crate::visibility::Eye;
use sjk_game_jka::damage::DamageRequest;
use sjk_game_jka::entity_pool::EntityPool;
use sjk_game_jka::force_dark::{OtherPlayer, OtherPlayers};
use sjk_game_jka::force_powers::{ForceFrame, ForcePowers, NUM_FORCE_POWERS};
use sjk_game_jka::npc_damage::NpcBlow;
use sjk_game_jka::npc_roster::{Fired, NpcRoster};
use sjk_game_jka::player_death::Rng;
use sjk_game_jka::pmove::{MovementCollision, MovementTrace};

/// `g_forceRegenTime`'s default: a point of Force every 200 ms.
const FORCE_REGEN_TIME: i32 = 200;
/// `CONTENTS_BODY`.
const CONTENTS_BODY: u32 = 0x100;
/// `PERS_HITS`, `PERS_ATTACKEE_ARMOR`.
const PERS_HITS: usize = 1;
const PERS_ATTACKEE_ARMOR: usize = 7;

/// What a player's grip, lightning and drain need beside the world, kept between updates
/// so that none allocates: the state its own is swapped with while it runs, the blows it
/// dealt other players, the others it touched with their states before, what its blows
/// on NPCs gave its hit counter and the names they fired.
pub(in crate::bridge) struct ForceScratch {
    state: PlayerState,
    blows: Vec<(u16, DamageRequest)>,
    touched: Vec<u16>,
    before: Vec<PlayerState>,
    hits: Vec<(i32, Option<u32>)>,
    fired: Fired,
}

impl Default for ForceScratch {
    fn default() -> Self {
        Self {
            state: PlayerState::zero(),
            blows: Vec::new(),
            touched: Vec::new(),
            before: Vec::new(),
            hits: Vec::new(),
            fired: Fired::new(),
        }
    }
}

impl NativeGame {
    /// `run` with `client`'s wire state, its Force and a [`ForceFrame`] over everyone else,
    /// the pool and the sounds ([`ServerOthers`]); a freed sound tracker tells everyone to
    /// stop its loop (`kls`). The player's state and Force are taken out of its peer while it
    /// runs, so that the others can be reached through the world. A blow on an NPC is dealt
    /// as it lands, through the roster; the blows on other players land afterwards, in
    /// order, and the others it changed restart their movement.
    pub(in crate::bridge) fn with_force_frame<T>(
        &mut self,
        client: usize,
        server_time: i32,
        run: impl FnOnce(&mut PlayerState, &mut ForcePowers, &mut ForceFrame) -> T,
    ) -> Option<T> {
        // Everyone the user's traces may strike, as they stand now.
        self.gather_obstacles(client);
        let since_last_frame = self.last_frame_time - self.previous_frame_time;
        let (saber_only, duel_fraglimit, gametype) = (
            self.settings.saber_only(),
            self.limits.duel_fraglimit,
            self.gametype,
        );
        // The Jedi Master's rule for the user's blows on an NPC, which is never the master.
        let spared_by_master = self.jedi_master_spares(Some(client as u16), usize::MAX);
        let mut scratch = std::mem::take(&mut self.force_scratch);
        let mut roster = std::mem::take(&mut self.npcs.roster);
        let map = self.map.take();
        let mut host = self.npc_host(map.as_ref(), None);
        let result = run_frame(
            &mut host,
            &mut roster,
            &mut scratch,
            client,
            server_time,
            (
                since_last_frame,
                saber_only,
                duel_fraglimit,
                gametype,
                spared_by_master,
            ),
            run,
        );
        let (telefrags, outcomes) = (
            std::mem::take(&mut host.telefrags),
            std::mem::take(&mut host.outcomes),
        );
        drop(host);
        self.npcs.roster = roster;
        self.map = map;
        let Some(result) = result else {
            self.force_scratch = scratch;
            return None;
        };
        // What the user's blows on NPCs gave its hit counter (`g_combat.c:4895-4905`).
        if let Some(peer) = self.peer_mut(client) {
            for (hits, armor) in scratch.hits.drain(..).filter(|(hits, _)| *hits != 0) {
                peer.state.persistent[PERS_HITS] =
                    (peer.state.persistent[PERS_HITS] as i32 + hits) as u32;
                peer.state.persistent[PERS_ATTACKEE_ARMOR] = armor.unwrap_or(0);
            }
        }
        scratch.hits.clear();
        // The others the powers changed restart their movement from their state.
        for (at, number) in scratch.touched.iter().enumerate() {
            let before = &scratch.before[at];
            let peer = self
                .players
                .at(usize::from(*number))
                .and_then(|handle| self.server.world_mut(self.world)?.entity_mut(handle));
            if let Some(peer) = peer
                && peer.state != *before
            {
                peer.movement = peer.movement.reseeded(&peer.state);
            }
        }
        for victim in telefrags {
            let request = DamageRequest {
                level_time: self.last_frame_time,
                attacker: None,
                direction: None,
                point: None,
                damage: 100_000,
                flags: sjk_game_jka::damage::DAMAGE_NO_PROTECTION,
                means: sjk_game_jka::means_of_death::MOD_TELEFRAG,
            };
            let _ = self.hurt(victim, request);
        }
        self.apply_npc_outcomes(outcomes);
        let fired = std::mem::take(&mut scratch.fired);
        self.fire_from_npcs(fired, server_time);
        // `G_Damage` for each blow on another player, in the order they were dealt.
        let blows = std::mem::take(&mut scratch.blows);
        self.force_scratch = scratch;
        for (victim, request) in &blows {
            let _ = self.strike(client, usize::from(*victim), *request, false);
        }
        self.force_scratch.blows = blows;
        self.force_scratch.blows.clear();
        Some(result.0)
    }
}

/// The update itself, over the host: the user's state and Force swapped out of its peer,
/// `run` over [`ServerOthers`], then put back.
fn run_frame<T>(
    host: &mut ServerHost<'_>,
    roster: &mut NpcRoster,
    scratch: &mut ForceScratch,
    client: usize,
    server_time: i32,
    (since_last_frame, saber_only, duel_fraglimit, gametype, spared_by_master): (
        i32,
        bool,
        i32,
        i32,
        bool,
    ),
    run: impl FnOnce(&mut PlayerState, &mut ForcePowers, &mut ForceFrame) -> T,
) -> Option<(T,)> {
    let ForceScratch {
        state,
        blows,
        touched,
        before,
        hits,
        fired,
    } = scratch;
    let world = host.server.world_mut(host.world)?;
    let peer = host
        .roster
        .at(client)
        .and_then(|handle| world.entity_mut(handle))?;
    std::mem::swap(&mut peer.state, state);
    let mut force = std::mem::replace(
        &mut peer.force,
        ForcePowers::with_levels([0; NUM_FORCE_POWERS]),
    );
    let (mut health, mut hand, mut invulnerable, team) = (
        peer.health,
        peer.knockdown.hand_extend_time,
        peer.invulnerable_until,
        peer.session.team,
    );
    let saber_style = peer.sabers.base_style();
    // `Cmd_ToggleSaber_f`'s off-sounds: each held saber's own, where it has one.
    let hands = &peer.sabers.hands;
    let saber_off_sounds = std::array::from_fn(|hand| match hands[hand].sound_off {
        index if index != 0 && (hand == 0 || hands[1].is_held()) => {
            sjk_game_jka::force_powers::SaberOffSound::Index(index)
        }
        _ => sjk_game_jka::force_powers::SaberOffSound::Silent,
    });
    let lone_regen = (gametype == super::super::GAMETYPE_POWERDUEL
        && peer.session.duel_team == sjk_game_jka::power_duel::DUELTEAM_LONE)
        .then(|| {
            sjk_game_jka::power_duel::lone_regen_time(
                FORCE_REGEN_TIME,
                peer.session.wins,
                duel_fraglimit,
            )
        });
    touched.clear();
    let mut others = ServerOthers {
        host,
        roster,
        client,
        spared_by_master,
        blows,
        touched,
        before,
        hits,
        fired,
    };
    let mut frame = ForceFrame {
        level_time: server_time,
        gametype,
        regen_time: FORCE_REGEN_TIME,
        since_last_frame,
        client: client as u16,
        npc: false,
        team,
        saber_style,
        saber_only,
        lone_regen,
        health: &mut health,
        hand_extend_time: &mut hand,
        invulnerable_until: &mut invulnerable,
        others: &mut others,
        saber_off_sounds,
    };
    let result = run(state, &mut force, &mut frame);
    let world = host.server.world_mut(host.world)?;
    let peer = host
        .roster
        .at(client)
        .and_then(|handle| world.entity_mut(handle))?;
    std::mem::swap(&mut peer.state, state);
    peer.force = force;
    (
        peer.health,
        peer.knockdown.hand_extend_time,
        peer.invulnerable_until,
    ) = (health, hand, invulnerable);
    Some((result,))
}

/// Everyone but a Force power's user on the server, for its grip, lightning and drain:
/// the players by their places and the begun NPCs by their numbers (the roster, taken out
/// of the game for the update), reached through the NPCs' host — the obstacles its traces
/// meet (gathered before the update, without the user; the NPCs' bodies among them), the
/// map, the pool its sounds, trackers and events become, the sound table, the generator.
struct ServerOthers<'h, 'a> {
    host: &'h mut ServerHost<'a>,
    roster: &'h mut NpcRoster,
    client: usize,
    spared_by_master: bool,
    blows: &'h mut Vec<(u16, DamageRequest)>,
    touched: &'h mut Vec<u16>,
    before: &'h mut Vec<PlayerState>,
    hits: &'h mut Vec<(i32, Option<u32>)>,
    fired: &'h mut Fired,
}

impl OtherPlayers for ServerOthers<'_, '_> {
    fn slots(&self) -> u16 {
        let npcs = self
            .roster
            .actors
            .iter()
            .map(|npc| npc.number + 1)
            .max()
            .unwrap_or(0);
        (self.host.roster.places() as u16).max(npcs)
    }

    fn player(&mut self, number: u16) -> Option<OtherPlayer<'_>> {
        if usize::from(number) == self.client {
            return None;
        }
        // An NPC: a begun one, standing or a corpse. Its movement restarts from its state
        // at its next move, so nothing is remembered of it.
        if let Some(at) = self
            .roster
            .actors
            .iter()
            .position(|npc| npc.number == number)
        {
            let npc = &mut self.roster.actors[at];
            return npc
                .begun()
                .then(|| sjk_game_jka::npc_force_update::npc_as_other(npc));
        }
        let handle = self.host.roster.at(usize::from(number))?;
        let peer = self
            .host
            .server
            .world_mut(self.host.world)?
            .entity_mut(handle)
            .filter(|peer| peer.begun && peer.body_active())?;
        // Its state as the update found it, to tell afterwards whether it changed.
        remember_touched(self.touched, self.before, number, &peer.state);
        let (absmin, absmax) = linked_box(peer);
        Some(OtherPlayer {
            state: &mut peer.state,
            force: &mut peer.force,
            health: &mut peer.health,
            knockdown: &mut peer.knockdown,
            other_killer: &mut peer.wounds.other_killer,
            npc_class: None,
            team: peer.session.team,
            absmin,
            absmax,
        })
    }

    fn trace(&mut self, start: [f32; 3], end: [f32; 3], pass: u16, mask: u32) -> MovementTrace {
        // The obstacles are without the user: only its own traces, or ones bodies do not
        // stop (a mind trick's line of sight), are asked for. The NPCs' bodies are among
        // them (`add_npc_bodies`).
        debug_assert!(usize::from(pass) == self.client || mask & CONTENTS_BODY == 0);
        let obstacles = self.host.solids;
        match self.host.map {
            Some(map) => WithPlayers {
                world: WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                },
                players: obstacles,
            }
            .trace(start, [0.0; 3], [0.0; 3], end, mask),
            None => WithPlayers {
                world: Void,
                players: obstacles,
            }
            .trace(start, [0.0; 3], [0.0; 3], end, mask),
        }
    }

    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool {
        self.host
            .map
            .is_none_or(|map| Eye::new(&map.bsp, &map.areas, from).sees_point(&map.bsp, to))
    }

    /// `G_Damage` as the power lands: on an NPC through the roster at once (its pain and
    /// death, the names they fire kept for the update's end), the user's hit counter
    /// credited afterwards; on a player, once the update is over.
    fn damage(&mut self, victim: u16, request: DamageRequest) {
        if !self.roster.is_npc(victim) {
            self.blows.push((victim, request));
            return;
        }
        let blow = NpcBlow {
            request,
            spared_by_master: self.spared_by_master,
            surface: None,
        };
        if let Some((hurt, mut fired)) =
            self.roster
                .damage(victim, blow, request.level_time, &mut *self.host)
        {
            self.hits.push((hurt.attacker_hits, hurt.attackee_armor));
            self.fired.append(&mut fired);
        }
    }

    fn rng(&mut self) -> &mut Rng {
        &mut self.host.deaths.rng
    }

    fn pool(&mut self) -> &mut EntityPool {
        self.host.pool
    }

    fn sound_index(&mut self, name: &[u8]) -> u16 {
        let told = &mut *self.host.told;
        self.host.sounds.index(name, &mut |index, value| {
            told.push(Told::ConfigString {
                index,
                previous: Vec::new(),
                value: value.to_vec(),
            })
        })
    }

    fn loop_stopped(&mut self, player: u16, tracker: u16) {
        self.host.told.push(Told::Everyone(
            format!("kls {player} {tracker}").into_bytes(),
        ));
    }
}
