//! The map's logic entities (OpenJK `codemp/game/g_trigger.c`, `g_target.c`): the timers,
//! counters and relays that decide *when* and *which* of a map's other entities are used,
//! and the targets that act on whoever set them off.
//!
//! | Classname | Reference |
//! | --- | --- |
//! | `func_timer` | `SP_func_timer`, `func_timer_use`, `func_timer_think` (`g_trigger.c:1771-1810`) |
//! | `target_random` | `SP_target_random`, `target_random_use` (`g_target.c:699-769`) |
//! | `target_counter` | `SP_target_counter`, `target_counter_use` (`g_target.c:629-691`) |
//! | `trigger_always` | `SP_trigger_always`, `trigger_always_think` (`g_trigger.c:893-905`) |
//! | `target_kill` | `SP_target_kill`, `target_kill_use` (`g_target.c:557-564`) |
//! | `target_teleporter` | `SP_target_teleporter`, `target_teleporter_use` (`g_target.c:463-488`) |
//! | `target_play_music` | `SP_target_play_music`, `target_play_music_use` (`g_target.c:990-1020`) |
//!
//! A logic entity never touches the world itself: a use or a think returns the [`Deed`]s it
//! asks of the level, in the reference's order, and the server carries them out — fires a
//! name (`G_UseTargets`), uses one entity of a name (`GlobalUse`), hurts, teleports or
//! changes the music. The game's own generator (`Q_irand`, `Q_flrand`) is the caller's.
//!
//! Who sets an entity off (an activator) is an entity identity: a client's is its slot,
//! anything else's the caller's own numbering, which must never collide with a slot. Only a
//! client activator is hurt or teleported.

use crate::player_death::Rng;
use sjk_entity::Entity;

/// `FRAMETIME`: a started timer's first think is a frame after the map's spawn.
const FRAMETIME: i32 = 100;
/// `trigger_always`' delay, "to make sure our use targets are present".
const ALWAYS_DELAY: i32 = 300;
/// `target_counter`'s spawnflag 128: deactivated once it has fired.
const COUNTER_DEACTIVATES: u32 = 128;

/// What a logic entity is, with what its spawn read.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// `func_timer`: fires its targets every `wait` seconds, give or take `random`.
    Timer { wait: f32, random: f32 },
    /// `target_random`: uses one of the entities its target names, at random; only once
    /// with spawnflag 1.
    Random,
    /// `target_counter`: fires after `count` uses (`target2` on each one before), then
    /// starts again `bounce` times (the reference reads no key for it: always 0).
    Counter {
        count: i32,
        initial: i32,
        bounce: i32,
    },
    /// `trigger_always`: fires its targets once, 300 ms into the level, and is gone.
    Always,
    /// `target_kill`: kills whoever set it off.
    Kill,
    /// `target_teleporter`: sends whoever set it off to one of the entities its target
    /// names (`G_PickTarget`).
    Teleporter,
    /// `target_play_music`: the level's music (`CS_MUSIC`) becomes `music`.
    PlayMusic { music: String },
}

/// One logic entity of the level.
#[derive(Clone, Debug, PartialEq)]
pub struct Logic {
    /// Its identity as an activator (`ent->s.number`): what a started timer fires for.
    pub number: usize,
    /// The classname it was spawned from, as the lump spells it.
    pub classname: String,
    /// `targetname`, `target` and `target2`.
    pub targetname: String,
    pub target: String,
    pub target2: String,
    pub spawnflags: u32,
    pub origin: [f32; 3],
    pub kind: Kind,
    /// `nextthink`: zero for none.
    pub next_think: i32,
    /// `activator`: whoever last set it off, `None` for nobody.
    pub activator: Option<usize>,
    /// `FL_INACTIVE` (`target_deactivate`, a counter's spawnflag 128): `GlobalUse` refuses it.
    pub inactive: bool,
    /// `ent->use`: a `target_random` with spawnflag 1 clears it once used; a
    /// `trigger_always` never has one.
    pub usable: bool,
    /// Freed (`G_FreeEntity`): a `trigger_always` after its think.
    pub freed: bool,
}

/// Why the reference refuses a logic entity at spawn.
#[derive(Clone, Debug, PartialEq)]
pub enum Refused {
    /// `target_play_music` without a `music` key: the reference stops the map
    /// (`ERR_DROP`); this server refuses the entity alone and says so.
    NoMusic { origin: [f32; 3] },
}

/// What a use or a think asks of the level, in the reference's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Deed {
    /// `G_UseTargets2(self, activator, name)`: every standing entity called `name` but
    /// the one firing is used.
    Fire {
        name: String,
        activator: Option<usize>,
    },
    /// `target_random`'s `GlobalUse`: the `pick`-th (from 1) standing entity called
    /// `name`, not counting the one firing, in entity order.
    FireOne {
        name: String,
        pick: usize,
        activator: Option<usize>,
    },
    /// `G_Damage(activator, NULL, NULL, NULL, NULL, 100000, DAMAGE_NO_PROTECTION,
    /// MOD_TELEFRAG)`: only an activator that takes damage (a client) is hurt.
    Kill { activator: usize },
    /// `TeleportPlayer(activator, dest->s.origin, dest->s.angles)` to a destination the
    /// caller picks among the entities called `destination` (`G_PickTarget`, C `rand()`);
    /// only a client activator (`if (!activator->client) return`).
    Teleport {
        activator: usize,
        destination: String,
    },
    /// `trap->SetConfigstring(CS_MUSIC, music)`.
    Music(String),
    /// The rest of `func_timer_think`, after the uses before it have run (they may draw
    /// from the generator first): the caller calls [`Logic::rearm`].
    Rearm,
}

impl Logic {
    /// The spawn function for `entity`, or `None` when it is no logic entity. `number` is
    /// its identity as an activator.
    pub fn spawn(entity: &Entity, number: usize, level_time: i32) -> Option<Result<Self, Refused>> {
        let classname = entity.classname()?;
        let text = |key: &str| entity.get(key).unwrap_or_default().to_owned();
        let float = |key: &str, default: f32| {
            entity
                .get(key)
                .map_or(default, |value| crate::text_parse::atof(value.as_bytes()))
        };
        let spawnflags = entity
            .get("spawnflags")
            .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()))
            as u32;
        let origin = crate::fx_runner::vector(entity, "origin");
        let mut logic = Self {
            number,
            classname: classname.to_owned(),
            targetname: text("targetname"),
            target: text("target"),
            target2: text("target2"),
            spawnflags,
            origin,
            kind: Kind::Kill,
            next_think: 0,
            activator: None,
            inactive: false,
            usable: true,
            freed: false,
        };
        match classname.to_ascii_lowercase().as_str() {
            "func_timer" => {
                let wait = float("wait", 1.0);
                let mut random = float("random", 1.0);
                if random >= wait {
                    // "was - FRAMETIME, but FRAMETIME is in msec (100) and these numbers are in *seconds*!"
                    random = wait - 1.0;
                }
                logic.kind = Kind::Timer { wait, random };
                if spawnflags & 1 != 0 {
                    logic.next_think = level_time + FRAMETIME;
                    logic.activator = Some(number);
                }
            }
            "target_random" => logic.kind = Kind::Random,
            "target_counter" => {
                let count = entity
                    .get("count")
                    .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()));
                let count = if count == 0 { 2 } else { count };
                logic.kind = Kind::Counter {
                    count,
                    initial: count,
                    bounce: 0,
                };
            }
            "trigger_always" => {
                logic.kind = Kind::Always;
                logic.usable = false;
                logic.next_think = level_time + ALWAYS_DELAY;
            }
            "target_kill" => logic.kind = Kind::Kill,
            "target_teleporter" => logic.kind = Kind::Teleporter,
            "target_play_music" => {
                let Some(music) = entity.get("music") else {
                    return Some(Err(Refused::NoMusic { origin }));
                };
                logic.kind = Kind::PlayMusic {
                    music: music.to_owned(),
                };
            }
            _ => return None,
        }
        Some(Ok(logic))
    }

    /// Whether `G_RunThink` runs its think at `level_time`.
    pub fn due(&self, level_time: i32) -> bool {
        !self.freed && self.next_think > 0 && self.next_think <= level_time
    }

    /// `G_RunThink`: its think, `nextthink` cleared first.
    pub fn think(&mut self) -> Vec<Deed> {
        self.next_think = 0;
        match self.kind {
            Kind::Timer { .. } => self.timer_fires(),
            Kind::Always => {
                // `G_UseTargets(ent, ent)`, then `G_FreeEntity`.
                self.freed = true;
                vec![Deed::Fire {
                    name: self.target.clone(),
                    activator: Some(self.number),
                }]
            }
            _ => Vec::new(),
        }
    }

    /// `GlobalUse` of this entity for `activator`. `named` is how many standing entities
    /// other than this one are called its `target`, which a `target_random` draws among.
    pub fn use_logic(
        &mut self,
        activator: Option<usize>,
        named: usize,
        rng: &mut Rng,
    ) -> Vec<Deed> {
        if self.inactive || !self.usable || self.freed {
            return Vec::new();
        }
        match &mut self.kind {
            Kind::Timer { .. } => {
                self.activator = activator;
                // On: off. Off: on, firing now.
                if self.next_think != 0 {
                    self.next_think = 0;
                    return Vec::new();
                }
                self.timer_fires()
            }
            Kind::Random => {
                if self.spawnflags & 1 != 0 {
                    self.usable = false;
                }
                match named {
                    0 => Vec::new(),
                    1 => vec![Deed::Fire {
                        name: self.target.clone(),
                        activator,
                    }],
                    _ => vec![Deed::FireOne {
                        name: self.target.clone(),
                        pick: rng.irand(1, named as i32) as usize,
                        activator,
                    }],
                }
            }
            Kind::Counter {
                count,
                initial,
                bounce,
            } => {
                if *count == 0 {
                    return Vec::new();
                }
                *count -= 1;
                if *count != 0 {
                    return if self.target2.is_empty() {
                        Vec::new()
                    } else {
                        vec![Deed::Fire {
                            name: self.target2.clone(),
                            activator,
                        }]
                    };
                }
                if self.spawnflags & COUNTER_DEACTIVATES != 0 {
                    self.inactive = true;
                }
                self.activator = activator;
                let fired = vec![Deed::Fire {
                    name: self.target.clone(),
                    activator,
                }];
                if *bounce != 0 {
                    *count = *initial;
                    if *bounce > 0 {
                        *bounce -= 1;
                    }
                }
                fired
            }
            Kind::Always => Vec::new(),
            Kind::Kill => activator
                .map(|activator| vec![Deed::Kill { activator }])
                .unwrap_or_default(),
            Kind::Teleporter => activator
                .map(|activator| {
                    vec![Deed::Teleport {
                        activator,
                        destination: self.target.clone(),
                    }]
                })
                .unwrap_or_default(),
            Kind::PlayMusic { music } => vec![Deed::Music(music.clone())],
        }
    }

    /// `func_timer_think`: its targets used for its activator, then [`Deed::Rearm`].
    fn timer_fires(&mut self) -> Vec<Deed> {
        vec![
            Deed::Fire {
                name: self.target.clone(),
                activator: self.activator,
            },
            Deed::Rearm,
        ]
    }

    /// The end of `func_timer_think`: the next firing `1000 * (wait + Q_flrand(-1, 1) *
    /// random)` on — reckoned in float, as the reference adds `level.time` to a float and
    /// truncates.
    pub fn rearm(&mut self, level_time: i32, rng: &mut Rng) {
        let Kind::Timer { wait, random } = self.kind else {
            return;
        };
        let spread = rng.flrand(-1.0, 1.0);
        self.next_think = (level_time as f32 + 1000.0 * (wait + spread * random)) as i32;
    }

    /// `count` as the transcripts print it (`ent->count`): a counter's remaining uses,
    /// zero for anything else.
    pub fn count(&self) -> i32 {
        match self.kind {
            Kind::Counter { count, .. } => count,
            _ => 0,
        }
    }
}
