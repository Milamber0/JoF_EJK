//! Siege on the server (`g_saga.c`): the round's rules are [`sjk_game_jka::siege`]'s, and
//! this is their [`SiegeWorld`] — the configstrings told, the round's broadcasts, the
//! scores, the targets used, the level's exit — plus the map's siege entities as the
//! game spawns them (`info_siege_objective`, `info_siege_decomplete`,
//! `info_siege_radaricon`, `target_siege_end`), what a name used reaches of them, and the
//! round's two moments in a frame: the respawn wave at its start and the timers after
//! the entities have run (`G_RunFrame`, `g_main.c:2935-2954` and `:3359`).
//!
//! The players' half — their sides, classes, respawns — is `bridge_siege_clients.rs`.

use super::*;
use sjk_game_jka::siege::{self, SiegePersistent, SiegePlayer, SiegeRound, SiegeUser, SiegeWorld};
use sjk_game_jka::siege_class::{SiegeRegistry, SiegeTeams};
use sjk_game_jka::siege_map::SiegeMapInfo;
use std::sync::Arc;

/// `EF_RADAROBJECT` (`bg_public.h:630`).
pub(super) const EF_RADAROBJECT: u32 = 1 << 2;
/// `ET_GENERAL`, `ET_EVENTS`.
const ET_GENERAL: u32 = 0;
const ET_EVENTS: u32 = 18;
/// Entity fields by their protocol-26 index: `genericenemyindex`, `owner`,
/// `trickedentindex`, `time`, `brokenLimbs`, `frame`.
const ES_ICON: usize = 18;
const ES_OWNER: usize = 40;
const ES_TRICKED: usize = 58;
const ES_BROKEN_LIMBS: usize = 79;
const ES_FRAME: usize = 83;
/// `SIEGEITEM_STARTOFFRADAR` (`g_saga.c:40`).
const STARTOFFRADAR: i32 = 8;

/// What each siege entity of the map is.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum SiegeEntityKind {
    /// `info_siege_objective`: objective `objective` of side `side`, and its own `target`.
    Objective {
        side: i32,
        objective: i32,
        target: Option<String>,
    },
    /// `info_siege_decomplete`.
    Decomplete { side: i32, objective: i32 },
    /// `info_siege_radaricon`.
    RadarIcon,
    /// `target_siege_end`.
    End,
}

/// One siege entity of the map, spawned.
#[derive(Clone, Debug)]
pub(super) struct SiegeEntity {
    /// Its entity, which every client is told of when it is on the radar.
    pub(super) number: EntityId,
    /// `targetname`, by which a use reaches it.
    pub(super) targetname: String,
    pub(super) kind: SiegeEntityKind,
}

/// An `info_player_siegeteam1`/`2` (`SP_info_player_siegeteam1`, `g_client.c:175-230`).
#[derive(Clone, Debug)]
pub(super) struct SiegePoint {
    /// Its side.
    pub(super) team: i32,
    pub(super) origin: [f32; 3],
    pub(super) angles: [f32; 3],
    /// `genericValue1`: only an enabled point is spawned on; `startoff` points start
    /// disabled and a use toggles them (`SiegePointUse`).
    pub(super) enabled: bool,
    /// `idealclass`: the class this point prefers.
    pub(super) ideal_class: Option<String>,
    pub(super) targetname: String,
    /// What a player spawning on it fires.
    pub(super) target: Option<String>,
}

/// A siege level: its round, the game's classes, the two sides, the map's siege
/// entities and its siege spawn points. Built as the level starts (`InitSiegeMode`), gone
/// when it ends.
pub(super) struct SiegeState {
    pub(super) round: SiegeRound,
    pub(super) registry: Arc<SiegeRegistry>,
    pub(super) sides: SiegeTeams,
    pub(super) entities: Vec<SiegeEntity>,
    pub(super) points: Vec<SiegePoint>,
    /// The carried and breakable objectives (`bridge_siege_items`).
    pub(super) items: Vec<super::bridge_siege_items::PlacedItem>,
}

/// What outlives a level: the engine's memory of the first round (`SiegePersSet`), and
/// the icons registered, whose configstrings stay (`G_IconIndex`).
#[derive(Default)]
pub(super) struct SiegeKeep {
    pub(super) persistent: SiegePersistent,
    pub(super) icons: sjk_game_jka::registries::IconTable,
}

impl NativeGame {
    /// `InitSiegeMode` and the siege entities' spawns, as a level starts: on a siege
    /// server, on a map with a `.siege` file its sides can be read from. Anything else
    /// leaves the level without siege, as `siege_valid` 0 does.
    pub(super) fn init_siege(&mut self, level_time: i32) {
        self.siege = None;
        if self.gametype != GAMETYPE_SIEGE {
            return;
        }
        let Some(files) = self.map.as_ref().and_then(|map| map.siege.clone()) else {
            return;
        };
        let overrides = [
            self.cvars.string(b"g_siegeTeam1"),
            self.cvars.string(b"g_siegeTeam2"),
        ];
        let overrides = overrides.map(|value| String::from_utf8_lossy(&value).into_owned());
        let info = match SiegeMapInfo::parse(&files.text, [&overrides[0], &overrides[1]]) {
            Ok(info) => info,
            Err(error) => {
                eprintln!("siege: the map's .siege file cannot be played: {error:?}");
                return;
            }
        };
        let sides = files
            .registry
            .sides(info.themes[0].as_deref(), info.themes[1].as_deref());
        let team_switch = self.cvars.integer(b"g_siegeTeamSwitch") != 0;
        let persistent = self.siege_keep.persistent;
        let mut strings = Vec::new();
        let round = SiegeRound::init(
            info,
            persistent,
            team_switch,
            level_time,
            &mut |index, value| strings.push((index, value.to_vec())),
        );
        for (index, value) in strings {
            self.set_config_string(index, &value);
        }
        self.siege = Some(Box::new(SiegeState {
            round,
            registry: files.registry,
            sides,
            entities: Vec::new(),
            points: Vec::new(),
            items: Vec::new(),
        }));
        self.spawn_siege_entities();
    }

    /// The map's siege entities in its order (`SP_info_siege_objective`,
    /// `SP_info_siege_decomplete`, `SP_info_siege_radaricon`, `SP_target_siege_end`).
    fn spawn_siege_entities(&mut self) {
        let Some(map) = self.map.take() else { return };
        for entity in &map.entities {
            if entity
                .classname()
                .is_some_and(|name| name.eq_ignore_ascii_case("misc_siege_item"))
            {
                if sjk_game_jka::bot_routes::spawns_in(entity, self.gametype) {
                    self.spawn_siege_item(entity);
                }
                continue;
            }
            let integer = |key: &str| {
                entity
                    .get(key)
                    .map_or(0, |text| sjk_game_jka::userinfo::atoi(text.as_bytes()))
            };
            let targetname = entity.get("targetname").unwrap_or("").to_owned();
            let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
            let mut state = EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
            state.set_raw_field(ES_ENTITY_TYPE, ET_GENERAL);
            for axis in 0..3 {
                state.set_raw_field(ES_POS_BASE[axis], origin[axis].to_bits());
            }
            if let Some(team @ 1..=2) = entity
                .classname()
                .and_then(|name| name.strip_prefix("info_player_siegeteam"))
                .and_then(|team| team.parse::<i32>().ok())
            {
                let angles = entity.vector("angles").ok().flatten().unwrap_or_else(|| {
                    [
                        0.0,
                        entity.number("angle").ok().flatten().unwrap_or_default(),
                        0.0,
                    ]
                });
                let point = SiegePoint {
                    team,
                    origin,
                    angles,
                    enabled: integer("startoff") == 0,
                    ideal_class: entity.get("idealclass").map(str::to_owned),
                    targetname: targetname.clone(),
                    target: entity.get("target").map(str::to_owned),
                };
                if let Some(siege) = self.siege.as_mut() {
                    siege.points.push(point);
                }
                continue;
            }
            let (kind, broadcast) = match entity.classname() {
                Some("info_siege_objective") => {
                    let (objective, side) = (integer("objective"), integer("side"));
                    if objective == 0 || side == 0 {
                        println!("ERROR: info_siege_objective without an objective or side value");
                        continue;
                    }
                    if integer("spawnflags") & STARTOFFRADAR == 0 {
                        state.set_raw_field(ES_EFLAGS, EF_RADAROBJECT);
                    }
                    if let Some(icon) = entity.get("icon").filter(|icon| !icon.is_empty()) {
                        let index = self.icon_index(icon.as_bytes());
                        state.set_raw_field(ES_ICON, u32::from(index));
                    }
                    state.set_raw_field(ES_BROKEN_LIMBS, side as u32);
                    state.set_raw_field(ES_FRAME, objective as u32);
                    (
                        SiegeEntityKind::Objective {
                            side,
                            objective,
                            target: entity.get("target").map(str::to_owned),
                        },
                        true,
                    )
                }
                Some("info_siege_decomplete") => {
                    let (objective, side) = (integer("objective"), integer("side"));
                    if objective == 0 || side == 0 {
                        println!(
                            "ERROR: info_siege_objective_decomplete without an objective or side value"
                        );
                        continue;
                    }
                    (SiegeEntityKind::Decomplete { side, objective }, false)
                }
                Some("info_siege_radaricon") => {
                    let on = integer("startoff") == 0;
                    if on {
                        state.set_raw_field(ES_EFLAGS, EF_RADAROBJECT);
                    }
                    // "that's the whole point of the entity": the reference drops the level
                    // for a radar icon without one; this server leaves it out.
                    let Some(icon) = entity.get("icon").filter(|icon| !icon.is_empty()) else {
                        eprintln!("siege: misc_siege_radaricon without an icon is left out");
                        continue;
                    };
                    let index = self.icon_index(icon.as_bytes());
                    state.set_raw_field(ES_ICON, u32::from(index));
                    (SiegeEntityKind::RadarIcon, on)
                }
                Some("target_siege_end") => (SiegeEntityKind::End, false),
                _ => continue,
            };
            // Only the objectives and the radar icons are linked (`trap->LinkEntity`); the
            // others are names a use reaches and nothing a client sees.
            let linked = matches!(
                kind,
                SiegeEntityKind::Objective { .. } | SiegeEntityKind::RadarIcon
            );
            let number = if linked {
                self.pool.spawn_entity(state, 0)
            } else {
                self.pool.spawn_hidden(0)
            };
            let Some(number) = number else { continue };
            if broadcast {
                self.pool.set_broadcast(number, true);
            }
            if let Some(siege) = self.siege.as_mut() {
                siege.entities.push(SiegeEntity {
                    number,
                    targetname,
                    kind,
                });
            }
        }
        self.map = Some(map);
    }

    /// `G_IconIndex`.
    pub(super) fn icon_index(&mut self, name: &[u8]) -> u16 {
        let mut strings = Vec::new();
        let index = self.siege_keep.icons.index(name, &mut |index, value| {
            strings.push((index, value.to_vec()))
        });
        for (index, value) in strings {
            self.publish_config_string(index, &value);
        }
        index
    }

    /// `G_UseTargets2` reaching the siege entities called `name`, used by entity
    /// `activator` (a client's number, or another entity's).
    pub(super) fn use_siege_entities(&mut self, name: &str, activator: usize, level_time: i32) {
        self.use_siege_items(name, level_time);
        let Some(siege) = self.siege.as_mut() else {
            return;
        };
        // `SiegePointUse`: a siege spawn point toggled on or off.
        for point in siege
            .points
            .iter_mut()
            .filter(|point| point.targetname == name)
        {
            point.enabled = !point.enabled;
        }
        let matching: Vec<usize> = siege
            .entities
            .iter()
            .enumerate()
            .filter(|(_, entity)| entity.targetname == name)
            .map(|(index, _)| index)
            .collect();
        for index in matching {
            let Some(entity) = self
                .siege
                .as_ref()
                .map(|siege| siege.entities[index].clone())
            else {
                return;
            };
            let activator_is_client =
                activator < self.players.places() && self.peer(activator).is_some();
            match entity.kind {
                SiegeEntityKind::Objective {
                    side,
                    objective,
                    target,
                } => {
                    let flags = self
                        .pool
                        .state(entity.number)
                        .and_then(|state| state.raw_field(ES_EFLAGS))
                        .unwrap_or(0);
                    let user = SiegeUser {
                        other: activator as i32,
                        activator: Some(activator as i32),
                        activator_is_client,
                    };
                    if siege::trigger_use(
                        self,
                        side,
                        objective,
                        flags & EF_RADAROBJECT != 0,
                        target.as_deref(),
                        user,
                        level_time,
                    ) {
                        self.set_entity_flags(entity.number, flags | EF_RADAROBJECT);
                    }
                }
                SiegeEntityKind::Decomplete { side, objective } => {
                    siege::decomplete(self, side, objective)
                }
                SiegeEntityKind::RadarIcon => {
                    // `SiegeIconUse`: toggled, and broadcast only while it is shown.
                    let flags = self
                        .pool
                        .state(entity.number)
                        .and_then(|state| state.raw_field(ES_EFLAGS))
                        .unwrap_or(0);
                    let on = flags & EF_RADAROBJECT == 0;
                    self.set_entity_flags(
                        entity.number,
                        if on {
                            flags | EF_RADAROBJECT
                        } else {
                            flags & !EF_RADAROBJECT
                        },
                    );
                    self.pool.set_broadcast(entity.number, on);
                }
                // `siegeEndUse`.
                SiegeEntityKind::End => self.siege_log_exit("Round ended", level_time),
            }
        }
    }

    /// `SelectSiegeSpawnPoint` (`g_team.c:1032-1166`) for a player about to spawn on its
    /// side: an enabled point of the side nobody stands on — one whose `idealclass` is the
    /// player's class if there is one — drawn with `rand()`; the side's first point when
    /// every one is taken; raised nine units. With it, what the point fires once the
    /// player is there. `None` for a spectator, or a side with no point at all (the
    /// ordinary spawn points then).
    pub(super) fn siege_spawn_place(
        &mut self,
        client: usize,
        command: &UserCommand,
        level_time: i32,
    ) -> Option<(SpawnPlace, Option<String>)> {
        let siege = self.siege.as_ref()?;
        let peer = self.peer(client)?;
        let team = peer.session.team;
        if team != 1 && team != 2 {
            return None;
        }
        let class = peer
            .siege_class_index
            .map(|index| siege.registry.classes[index].name.clone())
            .filter(|name| !name.is_empty());
        let occupied = |point: &SiegePoint| {
            self.obstacles.iter().any(|other| {
                other.model.is_none()
                    && usize::from(other.entity) < self.players.places()
                    && (0..3).all(|axis| {
                        point.origin[axis] + PLAYER_BOX.0[axis]
                            <= other.origin[axis] + other.bounds.1[axis] + 1.0
                            && point.origin[axis] + PLAYER_BOX.1[axis]
                                >= other.origin[axis] + other.bounds.0[axis] - 1.0
                    })
            })
        };
        let side: Vec<&SiegePoint> = siege
            .points
            .iter()
            .filter(|point| point.team == team)
            .collect();
        let first = *side.first()?;
        let free: Vec<&SiegePoint> = side
            .iter()
            .copied()
            .filter(|point| !occupied(point) && point.enabled)
            .take(32)
            .collect();
        let chosen = if free.is_empty() {
            first
        } else {
            let ideal: Vec<&SiegePoint> = free
                .iter()
                .copied()
                .filter(|point| {
                    class.as_deref().is_some_and(|class| {
                        point
                            .ideal_class
                            .as_deref()
                            .is_some_and(|ideal| ideal.eq_ignore_ascii_case(class))
                    })
                })
                .collect();
            let pool = if ideal.is_empty() { &free } else { &ideal };
            let roll = self.rand.next() as usize;
            pool[roll % pool.len()]
        };
        let origin = [chosen.origin[0], chosen.origin[1], chosen.origin[2] + 9.0];
        let place = SpawnPlace {
            origin,
            angles: chosen.angles,
            level_time,
            command_angles: command.angles,
        };
        Some((place, chosen.target.clone()))
    }

    /// `ClientSpawn`'s "fire the targets of the spawn point" (`g_client.c:3778-3779`), for
    /// a player spawned on a siege point.
    pub(super) fn siege_point_fired(
        &mut self,
        place: Option<(SpawnPlace, Option<String>)>,
        client: usize,
        level_time: i32,
    ) {
        if let Some((_, Some(target))) = place {
            self.fire_targets(&target, client, level_time);
        }
    }

    /// An entity's `eFlags`, set in the pool's state of it.
    fn set_entity_flags(&mut self, number: EntityId, flags: u32) {
        if let Some(mut state) = self.pool.state(number).cloned() {
            state.set_raw_field(ES_EFLAGS, flags);
            self.pool.set_state(number, &state);
        }
    }

    /// `LogExit(reason)` from a siege rule: the intermission queued now, as the exit rules
    /// queue it, and every client told at once through `CS_INTERMISSION`.
    pub(super) fn siege_log_exit(&mut self, reason: &str, level_time: i32) {
        if self.match_end.ending() {
            return;
        }
        self.match_end.queued = level_time;
        self.log_exit(reason);
        self.publish_config_string(sjk_game_jka::match_end::CS_INTERMISSION, b"1");
    }

    /// `G_RunFrame`'s respawn wave (`g_main.c:2935-2954`): every `g_siegeRespawn` seconds,
    /// each player waiting for it comes back.
    pub(super) fn siege_respawn_wave(&mut self, level_time: i32) {
        let wave = self.cvars.integer(b"g_siegeRespawn");
        let Some(siege) = self.siege.as_ref() else {
            return;
        };
        if wave == 0 || siege.round.respawn_check >= level_time {
            return;
        }
        for client in 0..self.players.places() {
            let waiting = self.peer(client).is_some_and(|peer| {
                peer.temp_spectate >= level_time && peer.session.team != i32::from(TEAM_SPECTATOR)
            });
            if waiting {
                // `ClientRespawn` while still waiting: no second body, then `SiegeRespawn`;
                // the wait is over (`tempSpectate = 0`).
                if let Some(peer) = self.peer_mut(client) {
                    peer.no_corpse = true;
                }
                self.leave_body(client, level_time);
                if let Some(peer) = self.peer_mut(client) {
                    peer.temp_spectate = 0;
                }
                self.siege_respawn_now(client, level_time);
            }
        }
        if let Some(siege) = self.siege.as_mut() {
            siege.round.respawn_check = level_time + wave * 1_000;
        }
    }

    /// `SiegeCheckTimers` (`g_main.c:3359`), after the frame's entities have run.
    pub(super) fn siege_timers(&mut self, level_time: i32) {
        if self.siege.is_none() {
            return;
        }
        let intermission = self.match_end.intermission_time != 0;
        siege::check_timers(self, level_time, intermission);
    }

    /// `imperial_attackers`/`rebel_attackers` for side `side` (0 or 1), which the bots read.
    pub(super) fn siege_attackers(&self, side: usize) -> bool {
        self.siege.as_ref().is_some_and(|siege| {
            siege
                .round
                .map
                .attackers
                .get(side)
                .is_some_and(|attackers| *attackers != 0)
        })
    }
}

impl SiegeWorld for NativeGame {
    fn round(&mut self) -> &mut SiegeRound {
        &mut self
            .siege
            .as_mut()
            .expect("a siege rule runs only on a siege level")
            .round
    }

    fn set_config_string(&mut self, index: usize, value: &[u8]) {
        NativeGame::set_config_string(self, index, value);
    }

    fn players(&self) -> Vec<SiegePlayer> {
        (0..self.players.places())
            .filter_map(|client| {
                let peer = self.peer(client)?;
                Some(SiegePlayer {
                    client: client as i32,
                    connected: peer.begun,
                    team: peer.session.team,
                    desired_team: peer.session.siege_desired_team,
                    following: peer.state.movement_flags() & PMF_FOLLOW != 0,
                })
            })
            .collect()
    }

    fn broadcast(&mut self, event: u32, parm: i32, weapon: i32, tricked: i32) {
        let mut state = EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
        state.set_raw_field(ES_ENTITY_TYPE, ET_EVENTS + event);
        state.set_raw_field(ES_EVENT_PARM, parm as u32);
        state.set_raw_field(ES_WEAPON, weapon as u32);
        state.set_raw_field(ES_TRICKED, tricked as u32);
        let level_time = self.last_frame_time;
        if let Some(number) = self.pool.spawn_temporary(state, level_time, None) {
            self.pool.set_broadcast(number, true);
        }
    }

    fn add_score(&mut self, client: i32, points: i32) {
        // `AddScore`: the player's score; siege scores no team (`g_combat.c:470-478`).
        if let Some(peer) = self.peer_mut(client as usize) {
            peer.state.persistent[PERS_SCORE] =
                (peer.state.persistent[PERS_SCORE] as i32 + points) as u32;
        }
        self.calculate_ranks();
    }

    fn use_targets(&mut self, activator: i32, name: &str) {
        let level_time = self.last_frame_time;
        self.fire_targets(name, activator.max(0) as usize, level_time);
    }

    fn log_exit(&mut self, reason: &str) {
        let level_time = self.last_frame_time;
        self.siege_log_exit(reason, level_time);
    }

    fn siege_respawn(&mut self, client: i32) {
        let level_time = self.last_frame_time;
        self.siege_respawn_now(client as usize, level_time);
    }

    fn set_persistent(&mut self, persistent: SiegePersistent) {
        self.siege_keep.persistent = persistent;
    }
}

/// `PMF_FOLLOW`.
const PMF_FOLLOW: u16 = 4_096;
/// `EV_SIEGESPEC`'s entity fields: its `time` (`g_siegeRespawnCheck`) and `owner`.
pub(super) fn siege_spec_event(origin: [f32; 3], client: usize, wave: i32) -> EntityState {
    let mut state = EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
    state.set_raw_field(ES_ENTITY_TYPE, ET_EVENTS + siege::EV_SIEGESPEC);
    for axis in 0..3 {
        state.set_raw_field(ES_POS_BASE[axis], origin[axis].to_bits());
    }
    state.set_raw_field(ES_TIME, wave as u32);
    state.set_raw_field(ES_OWNER, client as u32);
    state
}
