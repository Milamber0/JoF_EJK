//! cgame's own saber ignition and retraction sounds.
//!
//! The server voices a saber that is turned on or off while it stays the held
//! weapon: the toggle's `G_Sound` and `EV_SABER_UNHOLSTER` when an attack ignites
//! it. Drawing the saber from another weapon, or putting it away for one, is voiced
//! by cgame alone, from the player's own saber definitions (`saberInfo_t::soundOn`
//! and `soundOff`, `codemp/game/bg_public.h:1617-1619`): `CG_CheckPlayerG2Weapons`
//! for the local view's predicted state (`codemp/cgame/cg_weapons.c:2498-2580`,
//! called every frame by `CG_AddPacketEntities`, `cg_ents.c:3456`) and the weapon
//! block of `CG_Player` for every other player and a followed one
//! (`cg_players.c:8932-9001`). Both compare the weapon whose model is bolted to the
//! player (`centity_t::weapon`) with the one held now. `EV_CHANGE_WEAPON` gives the
//! saber no select sound for this reason (`cg_event.c:2024-2031`), so neither
//! transition has a server sound to double.
//!
//! The centity bookkeeping is mirrored with its quirks: a saber model taken off the
//! hand (melee and the emplaced gun bolt none, a thrown saber leaves it) forgets the
//! bolted weapon (`cg_players.c:8812-8816`); a saber caught again is silent
//! (`saberWasInFlight`, `:9003-9006,10044-10048`); a player who comes into view, or
//! respawns, already holding a saber is silent (`CG_ResetPlayerEntity`,
//! `:11265-11290`), but one who left view with a lit saber and returns with another
//! weapon retracts it; a changed hilt makes the local player's saber ignite again
//! (`CG_NewClientInfo`, `:1777-1808,1886-1990`); and the local view ignites a held
//! saber on its first frame of a map. A removed second saber keeps the default
//! sounds of `WP_RemoveSaber` (`bg_saberLoad.c:2187-2199`), so ignition also plays
//! `enemy_saber_on` for a single saber, as the server's toggle does
//! (`g_cmds.c` `Cmd_ToggleSaber_f`); retraction checks the second hilt's model.
//!
//! Each hilt's `soundOn`/`soundOff` plays, or the `WP_SaberSetDefaults` sound where
//! its definition authors none. A blade skin (an SJK unlockable,
//! [`super::saber_overrides`]) plays over them: a client wearing one also ignites and
//! retracts with its set's single `on` or `off`, once per switch, as its
//! `EV_SABER_UNHOLSTER` does. A client whose
//! clientinfo names no hilt (no real server sends one) keeps the stock `saberon`.

use super::*;
use crate::LegacyClientInfo;
use crate::pmove::MovementState;
use crate::saber_definitions::LegacySaberDefinition;
use sjk_protocol::PlayerState;
use std::collections::BTreeMap;

// codemp/game/bg_weapons.h:31-50.
const WP_NONE: u8 = 0;
const WP_MELEE: u8 = 2;
const WP_SABER: u8 = 3;
const WP_EMPLACED_GUN: u8 = 17;
const ET_PLAYER: u8 = 1; // bg_public.h:1246
const EF_DEAD: u32 = 1 << 1; // bg_public.h:627
const EF_TELEPORT_BIT: u32 = 1 << 3; // :632
const EF_NODRAW: u32 = 1 << 8; // :647
const PMF_FOLLOW: u16 = 4_096; // :478
const TEAM_SPECTATOR: u8 = 3; // bg_public.h:1073
const GT_TEAM: i32 = 6; // bg_public.h gametype_t
const CS_SERVERINFO: usize = 0;
const CS_PLAYERS: usize = 1_131;
const DEFAULT_SABER: &str = "Kyle"; // bg_public.h:42
/// `WP_SaberSetDefaults` ignition sound, kept by a definition without `soundOn`
/// and by a removed second saber (`bg_saberLoad.c:416`, `WP_RemoveSaber`).
const DEFAULT_SABER_ON: &str = "sound/weapons/saber/enemy_saber_on.wav";
/// `WP_SaberSetDefaults` retraction sound (`bg_saberLoad.c:418`).
const DEFAULT_SABER_OFF: &str = "sound/weapons/saber/enemy_saber_off.wav";
/// The stock ignition SJK has always played for `EV_SABER_UNHOLSTER`.
pub(super) const STOCK_SABER_ON: &str = "sound/weapons/saber/saberon.mp3";

/// The local view's weapon fields as cgame reads them every frame
/// (`cg.predictedPlayerState`): the predicted state in live play, the presented
/// snapshot's playerstate otherwise (demos, following another player).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacySaberView {
    pub client_num: u16,
    pub weapon: u8,
    pub saber_holstered: u8,
    pub saber_in_flight: bool,
    pub saber_entity_num: u16,
    pub entity_flags: u32,
    pub movement_flags: u16,
    /// `persistant[PERS_TEAM]`.
    pub team: u8,
    pub health: i32,
    pub origin: [f32; 3],
}

impl LegacySaberView {
    /// A snapshot's (or demo's) playerstate.
    pub fn from_player_state(player: &PlayerState) -> Self {
        Self {
            client_num: player.client_num(),
            weapon: player.weapon(),
            saber_holstered: player.saber_holstered(),
            saber_in_flight: player.saber_in_flight(),
            saber_entity_num: player.saber_entity_num(),
            entity_flags: player.entity_flags(),
            movement_flags: player.movement_flags(),
            team: player.team(),
            health: player.health(),
            origin: player.origin(),
        }
    }

    /// The live predicted state.
    pub fn from_movement_state(state: &MovementState) -> Self {
        Self {
            client_num: state.client_num,
            weapon: state.weapon,
            saber_holstered: state.saber_holstered,
            saber_in_flight: state.saber_in_flight,
            saber_entity_num: state.saber_entity_num,
            entity_flags: state.entity_flags,
            movement_flags: state.movement_flags,
            team: state.team,
            health: state.health,
            origin: state.origin,
        }
    }
}

/// A switch cgame voices: the saber drawn, or put away while lit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Switch {
    On,
    Off,
}

/// One client's two hands' sounds (`clientInfo_t::saber[0..1]`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Hands {
    pub(super) on: [Option<u16>; 2],
    pub(super) off: [Option<u16>; 2],
    /// `saber[1].model[0]`: a real second saber, not a removed slot.
    pub(super) second_model: bool,
}

impl Hands {
    /// The sounds a switch plays, in cgame's order.
    pub(super) fn sounds(self, switch: Switch) -> [Option<u16>; 2] {
        match switch {
            Switch::On => self.on,
            Switch::Off => [self.off[0], self.off[1].filter(|_| self.second_model)],
        }
    }
}

/// `CG_G2WeaponInstance`: whose model a weapon bolts on; `WP_NONE` has none.
fn instance(weapon: u8) -> Option<u8> {
    (weapon != WP_NONE).then_some(weapon)
}

/// The parts of one player's `centity_t` that decide the switch sounds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Centity {
    /// `centity_t::weapon`: the weapon whose model was last bolted on.
    weapon: u8,
    /// `centity_t::ghoul2weapon`: whose instance is bolted (`None` for NULL).
    instance: Option<u8>,
    /// A weapon model on the player's Ghoul2 slot 1 (`g2HasWeapon`).
    bolted: bool,
    /// `centity_t::saberWasInFlight`.
    was_in_flight: bool,
    /// `currentState.weapon` last seen, which `CG_NewClientInfo` reads.
    state_weapon: u8,
    /// Snapshot epoch this player was last present in, and its `eFlags` there.
    seen: u32,
    e_flags: u32,
}

/// One player's weapon fields as a frame reads them.
#[derive(Clone, Copy, Debug)]
struct Subject {
    weapon: u8,
    holstered: u8,
    in_flight: bool,
    saber_entity: u16,
    e_flags: u32,
}

impl Subject {
    fn entity(state: &EntityState) -> Self {
        Self {
            weapon: state.weapon(),
            holstered: state.saber_holstered(),
            in_flight: state.saber_in_flight(),
            // `saberEntityNum` is netfield 37 (codemp/qcommon/msg.cpp:884).
            saber_entity: state.event_sound_channel(),
            e_flags: state.e_flags(),
        }
    }

    /// `BG_PlayerStateToEntityState` derives `EF_DEAD` from health
    /// (`bg_misc.c`, after `saberHolstered`).
    fn view(view: LegacySaberView) -> Self {
        let dead = if view.health <= 0 { EF_DEAD } else { 0 };
        Self {
            weapon: view.weapon,
            holstered: view.saber_holstered,
            in_flight: view.saber_in_flight,
            saber_entity: view.saber_entity_num,
            e_flags: (view.entity_flags & !EF_DEAD) | dead,
        }
    }

    fn dead(self) -> bool {
        self.e_flags & EF_DEAD != 0
    }
}

impl Centity {
    /// `CG_CopyG2WeaponInstance` onto Ghoul2 slot 1 (`cg_weapons.c:2418-2494`):
    /// melee and the emplaced gun take the model off; `WP_NONE` has no instance
    /// and changes nothing.
    fn bolt(&mut self, weapon: u8) {
        if instance(weapon).is_some() {
            self.bolted = !matches!(weapon, WP_MELEE | WP_EMPLACED_GUN);
        }
    }

    /// Bolt the held weapon on, voicing the switch both callers check for.
    fn switch_weapon(&mut self, now: Subject) -> Option<Switch> {
        self.bolt(now.weapon);
        let switch = if self.weapon == WP_SABER && now.weapon != WP_SABER && now.holstered == 0 {
            Some(Switch::Off)
        } else if now.weapon == WP_SABER && self.weapon != WP_SABER && !self.was_in_flight {
            Some(Switch::On)
        } else {
            None
        };
        self.weapon = now.weapon;
        self.instance = instance(now.weapon);
        switch
    }

    /// `CG_CheckPlayerG2Weapons` for the local view when it follows no one.
    fn check_player_g2_weapons(&mut self, view: Subject, spectator: bool) -> Option<Switch> {
        if view.in_flight {
            self.instance = instance(WP_SABER);
        }
        if view.dead() {
            self.instance = None;
            return None;
        }
        if spectator {
            self.instance = None;
            self.weapon = WP_NONE;
            return None;
        }
        if self.instance == instance(view.weapon) {
            return None;
        }
        self.switch_weapon(view)
    }

    /// One `CG_Player` frame's weapon bookkeeping. `block` is false for the local
    /// view when it follows no one: `CG_CheckPlayerG2Weapons` has done its part.
    fn player_frame(&mut self, now: Subject, block: bool, spectator: bool) -> Option<Switch> {
        let had_weapon = self.bolted;
        if !had_weapon {
            self.instance = None;
            self.weapon = WP_NONE;
        }
        if now.e_flags & EF_NODRAW != 0 {
            return None;
        }
        if now.in_flight {
            self.instance = instance(WP_SABER);
        }
        let dead = now.dead();
        let mut switch = None;
        if block && !dead && self.instance != instance(now.weapon) {
            if spectator {
                self.instance = None;
                self.weapon = WP_NONE;
            } else {
                switch = self.switch_weapon(now);
            }
        } else if dead {
            self.instance = None;
        }
        if self.was_in_flight && had_weapon {
            self.was_in_flight = false;
        }
        if now.in_flight && had_weapon {
            // A lit saber thrown from the hand (`cg_players.c:10044-10071`);
            // knocked away or held by a freshly dead player (`:10525-10545`).
            let drawn = now.weapon == WP_SABER && now.holstered < 2 && !dead;
            if drawn {
                self.was_in_flight = true;
            }
            if (drawn && now.saber_entity != 0) || now.saber_entity == 0 || dead {
                self.bolted = false;
            }
        }
        switch
    }

    /// `CG_ResetPlayerEntity` for another player new to the snapshot or teleported:
    /// a saber already held is bolted on without a sound.
    fn reset_player_entity(&mut self, weapon: u8) {
        if weapon == WP_SABER && self.weapon != WP_SABER {
            self.weapon = WP_SABER;
            self.bolt(WP_SABER);
            self.instance = instance(WP_SABER);
        }
    }
}

/// Fixed per-client state behind cgame's saber switch sounds.
pub(super) struct SaberSwitchSounds {
    hands: [Hands; MAX_CLIENTS],
    saber_keys: [u64; MAX_CLIENTS],
    body_keys: [u64; MAX_CLIENTS],
    centities: [Centity; MAX_CLIENTS],
    epoch: u32,
    snapshot_time: Option<i32>,
    /// `cg.predictedPlayerState.clientNum` at the latest frame.
    view_client: u16,
    switches: [(u16, Switch, [f32; 3]); MAX_CLIENTS],
    switch_count: usize,
}

impl SaberSwitchSounds {
    /// Resolve every client's hands at gamestate time. Every centity starts
    /// cleared, as `CG_Init` leaves them.
    pub(super) fn new(
        game_state: &GameState,
        definitions: &BTreeMap<String, LegacySaberDefinition>,
        intern: &mut impl FnMut(&str) -> u16,
    ) -> Self {
        let team_game = team_game(game_state);
        let config = |client: usize| {
            game_state
                .config_string(CS_PLAYERS + client)
                .unwrap_or_default()
        };
        Self {
            hands: std::array::from_fn(|client| hands(config(client), definitions, intern)),
            saber_keys: std::array::from_fn(|client| saber_key(config(client))),
            body_keys: std::array::from_fn(|client| body_key(config(client), team_game)),
            centities: [Centity::default(); MAX_CLIENTS],
            epoch: 1,
            snapshot_time: None,
            view_client: u16::try_from(game_state.client_num).unwrap_or(0),
            switches: [(0, Switch::On, [0.0; 3]); MAX_CLIENTS],
            switch_count: 0,
        }
    }

    /// `CG_NewClientInfo` for one changed `CS_PLAYERS` string.
    pub(super) fn refresh_client(
        &mut self,
        game_state: &GameState,
        client: usize,
        definitions: &BTreeMap<String, LegacySaberDefinition>,
        intern: &mut impl FnMut(&str) -> u16,
    ) {
        let config = game_state
            .config_string(CS_PLAYERS + client)
            .unwrap_or_default();
        let (saber, body) = (saber_key(config), body_key(config, team_game(game_state)));
        self.hands[client] = hands(config, definitions, intern);
        let (old_saber, old_body) = (self.saber_keys[client], self.body_keys[client]);
        (self.saber_keys[client], self.body_keys[client]) = (saber, body);
        if config.is_empty() {
            // "player just left": only the clientinfo is cleared (`cg_players.c:1561-1577`).
            return;
        }
        // "force a weapon change anyway, for all clients" (`:1874-1879`).
        for centity in &mut self.centities {
            centity.instance = None;
        }
        let centity = &mut self.centities[client];
        if saber != old_saber {
            // A new hilt forces a refresh from no weapon (`:1777-1808`).
            centity.weapon = WP_NONE;
        }
        if body != old_body {
            // A new body instance has no weapon bolted (`:1886-1935`); another
            // player's held saber is bolted straight back on (`:1963-1990`).
            centity.bolted = false;
            if client != usize::from(self.view_client) && centity.state_weapon == WP_SABER {
                centity.weapon = WP_SABER;
                centity.bolt(WP_SABER);
                centity.instance = instance(WP_SABER);
            }
        }
    }

    /// One rendered frame: the local view, then every other player in the
    /// presented snapshot, in `CG_AddPacketEntities` order.
    pub(super) fn observe(
        &mut self,
        snapshot: &Snapshot,
        view: LegacySaberView,
        teams: &[u8; MAX_CLIENTS],
    ) {
        self.switch_count = 0;
        self.view_client = view.client_num;
        if self.snapshot_time != Some(snapshot.server_time) {
            self.transition(snapshot);
        }
        let local = usize::from(view.client_num);
        if local < MAX_CLIENTS {
            let now = Subject::view(view);
            let follow = view.movement_flags & PMF_FOLLOW != 0;
            let spectator = teams[local] == TEAM_SPECTATOR;
            let centity = &mut self.centities[local];
            centity.state_weapon = view.weapon;
            let checked = if follow {
                None
            } else {
                centity.check_player_g2_weapons(now, spectator || view.team == TEAM_SPECTATOR)
            };
            let drawn = centity.player_frame(now, follow, spectator);
            for switch in [checked, drawn].into_iter().flatten() {
                self.push(view.client_num, switch, view.origin);
            }
        }
        for state in &snapshot.entities {
            let Some(client) = player_slot(state) else {
                continue;
            };
            // "Don't re-add ents that have been predicted" (`cg_ents.c:3484-3487`).
            if state.number() == snapshot.player.client_num() {
                continue;
            }
            let centity = &mut self.centities[client];
            centity.state_weapon = state.weapon();
            let spectator = teams[client] == TEAM_SPECTATOR;
            if let Some(switch) = centity.player_frame(Subject::entity(state), true, spectator) {
                self.push(state.number(), switch, state.trajectory_base());
            }
        }
    }

    /// `CG_TransitionSnapshot`: a player absent from the previous snapshot or
    /// teleported is not interpolated and goes through `CG_ResetEntity`.
    fn transition(&mut self, snapshot: &Snapshot) {
        self.snapshot_time = Some(snapshot.server_time);
        let previous = self.epoch;
        self.epoch = self.epoch.wrapping_add(1).max(1);
        for state in &snapshot.entities {
            let Some(client) = player_slot(state) else {
                continue;
            };
            let centity = &mut self.centities[client];
            let interpolate = centity.seen == previous
                && (centity.e_flags ^ state.e_flags()) & EF_TELEPORT_BIT == 0;
            centity.seen = self.epoch;
            centity.e_flags = state.e_flags();
            if !interpolate && state.number() != self.view_client {
                centity.reset_player_entity(state.weapon());
            }
        }
    }

    fn push(&mut self, client: u16, switch: Switch, origin: [f32; 3]) {
        if let Some(slot) = self.switches.get_mut(self.switch_count) {
            *slot = (client, switch, origin);
            self.switch_count += 1;
        }
    }

    pub(super) fn switches(&self) -> &[(u16, Switch, [f32; 3])] {
        &self.switches[..self.switch_count]
    }

    /// A client's hands; none outside the client range.
    pub(super) fn hands(&self, client: u16) -> Hands {
        self.hands
            .get(usize::from(client))
            .copied()
            .unwrap_or_default()
    }
}

impl LegacySoundAdapter {
    /// Voice cgame's saber ignition and retraction on a weapon switch, once per
    /// rendered frame like `CG_AddPacketEntities`, from the presented snapshot and
    /// the local view (`cg.predictedPlayerState`). Replaces [`Self::decisions`].
    pub fn observe_saber_switches(&mut self, snapshot: &Snapshot, view: LegacySaberView) -> usize {
        self.decisions.clear();
        self.saber_switch
            .observe(snapshot, view, &self.client_teams);
        for index in 0..self.saber_switch.switches().len() {
            let (client, switch, origin) = self.saber_switch.switches()[index];
            let event = match switch {
                Switch::On => LegacySoundEvent::SaberSwitchOn,
                Switch::Off => LegacySoundEvent::SaberSwitchOff,
            };
            let sounds = self.switch_sounds(client, switch);
            self.emit_hands(event, sounds, client, origin, snapshot);
        }
        self.decisions.len()
    }

    /// What `client` plays for `switch`: each hand's own (`soundOn`/`soundOff` or their
    /// defaults), then its blade skin's one sound on top when it wears one.
    fn switch_sounds(&self, client: u16, switch: Switch) -> [Option<u16>; 3] {
        let skin = match switch {
            Switch::On => self.saber_overrides.on(client),
            Switch::Off => self.saber_overrides.off(client),
        };
        let [first, second] = self.saber_switch.hands(client).sounds(switch);
        [first, second, skin]
    }

    /// `EV_SABER_UNHOLSTER` from a client: each hand's `soundOn`
    /// (`cg_event.c:2381-2407`), and the blade skin's ignition over them.
    pub(super) fn emit_unholster(&mut self, client: u16, origin: [f32; 3], snapshot: &Snapshot) {
        let sounds = self.switch_sounds(client, Switch::On);
        self.emit_hands(
            LegacySoundEvent::SaberUnholster,
            sounds,
            client,
            origin,
            snapshot,
        );
    }

    /// `S_StartSound(origin, number, CHAN_AUTO, sound)` for each set hand.
    fn emit_hands(
        &mut self,
        event: LegacySoundEvent,
        sounds: [Option<u16>; 3],
        client: u16,
        origin: [f32; 3],
        snapshot: &Snapshot,
    ) {
        let mut additional = false;
        for sound in sounds.into_iter().flatten() {
            self.emit(
                event,
                Some(sound),
                client,
                CHAN_AUTO,
                false,
                additional,
                origin,
                snapshot,
            );
            additional = true;
        }
    }
}

/// A player entity's client slot.
fn player_slot(state: &EntityState) -> Option<usize> {
    let number = usize::from(state.number());
    (state.entity_type() == ET_PLAYER && number < MAX_CLIENTS).then_some(number)
}

fn team_game(game_state: &GameState) -> bool {
    config_info_value(
        game_state.config_string(CS_SERVERINFO).unwrap_or_default(),
        "g_gametype",
    )
    .and_then(|value| value.parse::<i32>().ok())
    .is_some_and(|gametype| gametype >= GT_TEAM)
}

/// Both hands' raw definition names, `none` included (`st`/`st2`, or the long
/// spellings [`crate::player_identity`] prefers).
fn raw_saber_names(config: &[u8]) -> [Option<&str>; 2] {
    let info = LegacyClientInfo::new(config);
    let value = |long: &str, compact: &str| {
        let present = |key| info.text(key).filter(|value| !value.is_empty());
        present(long).or_else(|| present(compact))
    };
    [value("saber1", "st"), value("saber2", "st2")]
}

/// `WP_SetSaber` removes a second saber named `none` or `remove`.
fn removes(name: &str) -> bool {
    name.eq_ignore_ascii_case("none") || name.eq_ignore_ascii_case("remove")
}

/// The definition `WP_SetSaber` loads: a `notInMP` hilt or an unknown name loads
/// `Kyle` instead (`bg_saberLoad.c:2227-2235,2033-2054`); with no `Kyle` either the
/// `WP_SaberSetDefaults` values stand.
fn definition<'a>(
    definitions: &'a BTreeMap<String, LegacySaberDefinition>,
    name: &str,
) -> Option<&'a LegacySaberDefinition> {
    definitions
        .get(&name.to_ascii_lowercase())
        .filter(|definition| !definition.not_in_mp)
        .or_else(|| definitions.get(&DEFAULT_SABER.to_ascii_lowercase()))
}

/// Both hands as `CG_NewClientInfo` loads them (`cg_players.c:1745-1773`).
fn hands(
    config: &[u8],
    definitions: &BTreeMap<String, LegacySaberDefinition>,
    intern: &mut impl FnMut(&str) -> u16,
) -> Hands {
    if config.is_empty() {
        return Hands::default();
    }
    let [first, second] = raw_saber_names(config);
    let mut hands = Hands::default();
    let Some(first) = first else {
        // No hilt named: the stock ignition SJK played before it read hilts.
        hands.on[0] = Some(intern(STOCK_SABER_ON));
        return hands;
    };
    let mut sound = |path: &str| (!path.is_empty()).then(|| intern(path));
    // The first saber can never be removed; `none` leaves it unloaded.
    let first = (!removes(first)).then(|| definition(definitions, first));
    let first_two_handed = first.flatten().is_some_and(|hilt| hilt.two_handed);
    if let Some(first) = first {
        let (on, off) = sound_paths(first);
        hands.on[0] = sound(on);
        hands.off[0] = sound(off);
    }
    let Some(second) = second else {
        return hands;
    };
    let loaded = (!removes(second)).then(|| definition(definitions, second));
    // A two-handed hilt is never a second saber, and keeps none beside it
    // (`bg_saberLoad.c:2237-2247`).
    let kept = loaded.filter(|hilt| !first_two_handed && !hilt.is_some_and(|hilt| hilt.two_handed));
    let (on, off) = sound_paths(kept.flatten());
    hands.on[1] = sound(on);
    hands.off[1] = sound(off);
    hands.second_model = kept.is_some();
    hands
}

/// A loaded hand's `soundOn` and `soundOff`, or the `WP_SaberSetDefaults` ones
/// where it authors none.
fn sound_paths(definition: Option<&LegacySaberDefinition>) -> (&str, &str) {
    let on = definition.and_then(|hilt| hilt.sound_on.as_deref());
    let off = definition.and_then(|hilt| hilt.sound_off.as_deref());
    (
        on.unwrap_or(DEFAULT_SABER_ON),
        off.unwrap_or(DEFAULT_SABER_OFF),
    )
}

fn fold_lowercase(hash: u64, text: &str) -> u64 {
    text.bytes().chain(*b"\\").fold(hash, |hash, byte| {
        (hash ^ u64::from(byte.to_ascii_lowercase())).wrapping_mul(0x100_0000_01b3)
    })
}

/// The hilt names, compared case-insensitively as `CG_NewClientInfo` does.
fn saber_key(config: &[u8]) -> u64 {
    raw_saber_names(config)
        .into_iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, name| {
            fold_lowercase(hash, name.unwrap_or_default())
        })
}

/// What `CG_ScanForExistingClientInfo` matches a clientinfo by: model and skin,
/// both hilts and, in team games, the team. A change gives the player a new
/// Ghoul2 instance.
fn body_key(config: &[u8], team_game: bool) -> u64 {
    let info = LegacyClientInfo::new(config);
    let model = fold_lowercase(saber_key(config), info.text("model").unwrap_or_default());
    if team_game {
        fold_lowercase(model, info.text("t").unwrap_or_default())
    } else {
        model
    }
}

#[cfg(test)]
#[path = "sound_saber_switch_tests.rs"]
mod tests;
