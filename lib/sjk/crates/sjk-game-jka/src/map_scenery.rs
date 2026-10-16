//! What a map places for its clients to draw rather than for its players to touch
//! (OpenJK `codemp/game/g_misc.c`, `g_mover.c`): the world's weather effects, the sky
//! portal, the portal surfaces and their cameras, and breakable glass.
//!
//! | Classname | Reference | Here |
//! | --- | --- | --- |
//! | `fx_snow`, `fx_rain`, `fx_wind`, `fx_spacedust` | `SP_CreateSnow`, `SP_CreateRain`, `SP_CreateWind`, `SP_CreateSpaceDust` (`g_misc.c:2555-2690`): effect names registered in `CS_EFFECTS`, nothing linked | [`weather_effects`] |
//! | `misc_weather_zone`, `misc_skyportal_orient` | freed at spawn (clients read the zones from the map itself) | the spawn table's `Removed` |
//! | `misc_skyportal` | `SP_misc_skyportal` (`CS_SKYBOXORG`), `G_PortalifyEntities` 1050 ms on | [`sky_portal`] |
//! | `misc_portal_surface`, `misc_portal_camera` | `SP_misc_portal_surface`, `locateCamera`, `SP_misc_portal_camera` | [`PortalSurface`], [`camera_roll`] |
//! | `func_glass` | `SP_func_glass`, `GlassDie`, `GlassUse`, `G_Damage`'s `SVF_GLASS_BRUSH` | [`Glass`] |

use sjk_entity::Entity;

/// `ET_PORTAL`.
pub const ET_PORTAL: u32 = 8;
/// `EV_GLASS_SHATTER`.
pub const EV_GLASS_SHATTER: u32 = 81;
/// `CS_SKYBOXORG` (`CS_MODELS + MAX_MODELS`, the one before `CS_SOUNDS`).
pub const CS_SKYBOXORG: usize = crate::registries::CS_SOUNDS - 1;
/// When `G_PortalifyEntities` runs, after the sky portal's spawn: "give it some time first
/// so that all other entities are spawned".
pub const PORTALIFY_DELAY: i32 = 1_050;
/// When `locateCamera` runs after a portal surface's spawn.
pub const LOCATE_CAMERA_DELAY: i32 = 100;

/// The effect names a weather entity registers, in the order `G_EffectIndex` is called;
/// empty for any other entity.
pub fn weather_effects(entity: &Entity) -> Vec<String> {
    let Some(classname) = entity.classname() else {
        return Vec::new();
    };
    let spawnflags = entity
        .get("spawnflags")
        .map_or(0, |value| crate::userinfo::atoi(value.as_bytes())) as u32;
    let mut names = Vec::new();
    match classname.to_ascii_lowercase().as_str() {
        "fx_snow" => {
            names.extend(["*snow", "*fog", "*constantwind ( 100 100 -100 )"].map(str::to_owned))
        }
        "fx_spacedust" => {
            let count = entity
                .get("count")
                .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()));
            names.push(format!("*spacedust {count}"));
        }
        "fx_rain" => {
            if spawnflags == 0 {
                names.push("*rain".to_owned());
                return names;
            }
            if spawnflags & 1 != 0 {
                names.push("*lightrain".to_owned());
            } else if spawnflags & 2 != 0 {
                names.push("*rain".to_owned());
            } else if spawnflags & 4 != 0 {
                names.extend(["*heavyrain", "*heavyrainfog"].map(str::to_owned));
            } else if spawnflags & 8 != 0 {
                names.extend(["world/acid_fizz", "*acidrain"].map(str::to_owned));
            }
            if spawnflags & 32 != 0 {
                names.push("*fog".to_owned());
            }
        }
        "fx_wind" => {
            if spawnflags & 1 != 0 {
                names.push("*wind".to_owned());
            }
            if spawnflags & 2 != 0 {
                let (forward, _) =
                    crate::pmove::flight::flight_axes(crate::fx_runner::spawn_angles(entity));
                let speed = entity
                    .get("speed")
                    .map_or(500.0, |value| crate::text_parse::atof(value.as_bytes()));
                let wind = [forward.x * speed, forward.y * speed, forward.z * speed];
                names.push(format!(
                    "*constantwind ( {:.6} {:.6} {:.6} )",
                    wind[0], wind[1], wind[2]
                ));
            }
            if spawnflags & 4 != 0 {
                names.push("*gustingwind".to_owned());
            }
            if spawnflags & 32 != 0 {
                names.push("*fog".to_owned());
            }
            if spawnflags & 64 != 0 {
                names.push("*light_fog".to_owned());
            }
        }
        _ => {}
    }
    names
}

/// `SP_misc_skyportal`'s `CS_SKYBOXORG`: the portal's origin, its `fov` (80), whether any of
/// `fogcolor`, `fognear`, `fogfar` is set (counted: `isfog += G_SpawnVector(...)`), then those
/// three (defaults `0 0 0`, 0, 300) — printed as `"%.2f %.2f %.2f %.1f %i %.2f %.2f %.2f %i %i"`.
pub fn sky_portal(entity: &Entity) -> Vec<u8> {
    let origin = crate::fx_runner::vector(entity, "origin");
    let fov = entity
        .get("fov")
        .map_or(80.0, |value| crate::text_parse::atof(value.as_bytes()));
    let fog_set = ["fogcolor", "fognear", "fogfar"]
        .iter()
        .filter(|key| entity.get(key).is_some())
        .count();
    let color = entity
        .get("fogcolor")
        .map_or([0.0; 3], |_| crate::fx_runner::vector(entity, "fogcolor"));
    let near = entity
        .get("fognear")
        .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()));
    let far = entity
        .get("fogfar")
        .map_or(300, |value| crate::userinfo::atoi(value.as_bytes()));
    format!(
        "{:.2} {:.2} {:.2} {:.1} {} {:.2} {:.2} {:.2} {} {}",
        origin[0], origin[1], origin[2], fov, fog_set, color[0], color[1], color[2], near, far
    )
    .into_bytes()
}

/// `SP_misc_portal_camera`'s `s.clientNum = roll / 360.0 * 256` (the `roll` key, 0).
pub fn camera_roll(entity: &Entity) -> u32 {
    let roll = entity
        .get("roll")
        .map_or(0.0, |value| crate::text_parse::atof(value.as_bytes()));
    (f64::from(roll) / 360.0 * 256.0) as i32 as u32
}

/// A portal surface: `ET_PORTAL`, `SVF_PORTAL`, what a client draws through it.
#[derive(Clone, Debug, PartialEq)]
pub struct PortalSurface {
    pub origin: [f32; 3],
    /// `target`: its camera, found by `locateCamera` a tenth of a second after the spawn.
    pub target: String,
    /// `s.origin2`: where the view through it is from (its own origin when it names no
    /// camera; the camera's once `locateCamera` found it).
    pub origin2: [f32; 3],
    /// `s.frame` (the camera's rotate speed: 25, 75), `s.powerups` (1: the camera swings),
    /// `s.clientNum` (its roll), `s.eventParm` (`DirToByte` of where the camera looks).
    pub frame: u32,
    pub powerups: u32,
    pub client_num: u32,
    pub event_parm: u32,
    /// `nextthink` for `locateCamera`; 0 once done.
    pub locate_at: i32,
}

/// The camera a portal surface looks through, as `locateCamera` reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub spawnflags: u32,
    /// Its `s.clientNum` ([`camera_roll`]).
    pub roll: u32,
    /// Where its own `target` is, if it names one that stands.
    pub aim: Option<[f32; 3]>,
}

impl PortalSurface {
    /// `SP_misc_portal_surface`.
    pub fn spawn(entity: &Entity, level_time: i32) -> Self {
        let origin = crate::fx_runner::vector(entity, "origin");
        let target = entity.get("target").unwrap_or_default().to_owned();
        let locate_at = if target.is_empty() {
            0
        } else {
            level_time + LOCATE_CAMERA_DELAY
        };
        // Only a surface with no camera looks from where it stands.
        let origin2 = if target.is_empty() { origin } else { [0.0; 3] };
        Self {
            origin,
            target,
            origin2,
            frame: 0,
            powerups: 0,
            client_num: 0,
            event_parm: 0,
            locate_at,
        }
    }

    /// `locateCamera` with the camera `G_PickTarget` chose; `None` when there is none, which
    /// frees the surface ("Couldn't find target for misc_partal_surface").
    pub fn locate(&mut self, camera: Option<Camera>) -> bool {
        self.locate_at = 0;
        let Some(camera) = camera else { return false };
        if camera.spawnflags & 1 != 0 {
            self.frame = 25;
        } else if camera.spawnflags & 2 != 0 {
            self.frame = 75;
        }
        self.powerups = u32::from(camera.spawnflags & 4 == 0);
        self.client_num = camera.roll;
        self.origin2 = camera.origin;
        let direction = match camera.aim {
            Some(aim) => {
                let mut direction = std::array::from_fn(|axis| aim[axis] - camera.origin[axis]);
                crate::player_angle_math::normalize(&mut direction);
                direction
            }
            None => crate::movers::movedir(camera.angles),
        };
        self.event_parm = u32::from(sjk_protocol::legacy_direction_to_byte(direction));
        true
    }
}

/// A `func_glass`: a brush that shatters at the first blow (`health` 1 unless the map sets
/// it) or when used.
#[derive(Clone, Debug, PartialEq)]
pub struct Glass {
    pub model: usize,
    pub bounds: ([f32; 3], [f32; 3]),
    pub health: i32,
    /// `maxshards` (`genericValue3`), which the shatter event carries.
    pub max_shards: i32,
    /// `takedamage`: spawnflag 1 makes it unbreakable by damage.
    pub takes_damage: bool,
    pub targetname: String,
    pub target: String,
    /// `genericValue5`: already shattered.
    pub shattered: bool,
}

/// `GlassDie`'s event (`G_TempEntity(dif, EV_GLASS_SHATTER)`): where, whose, and the shards.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shatter {
    /// `dif`: the middle of its box.
    pub at: [f32; 3],
    /// `s.origin` = `pos1` (where the blow landed), `s.angles` = `pos2` (its direction).
    pub point: [f32; 3],
    pub direction: [f32; 3],
    /// `s.trickedentindex` = `splashRadius` = 40, `s.pos.trTime` = `maxshards`.
    pub radius: u32,
    pub shards: i32,
}

impl Glass {
    /// `SP_func_glass` (`bounds` its inline model's).
    pub fn spawn(entity: &Entity, bounds: ([f32; 3], [f32; 3])) -> Option<Self> {
        if !entity.classname()?.eq_ignore_ascii_case("func_glass") {
            return None;
        }
        let model = entity.get("model")?.strip_prefix('*')?.parse().ok()?;
        let int = |key: &str| {
            entity
                .get(key)
                .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()))
        };
        let health = int("health");
        Some(Self {
            model,
            bounds,
            health: if health == 0 { 1 } else { health },
            max_shards: int("maxshards"),
            takes_damage: int("spawnflags") & 1 == 0,
            targetname: entity.get("targetname").unwrap_or_default().to_owned(),
            target: entity.get("target").unwrap_or_default().to_owned(),
            shattered: false,
        })
    }

    /// The middle of its box (`(r.absmax + r.absmin) / 2`: a brush model's bounds are where
    /// the map drew it).
    pub fn center(&self) -> [f32; 3] {
        std::array::from_fn(|axis| {
            ((self.bounds.1[axis] + 1.0) + (self.bounds.0[axis] - 1.0)) / 2.0
        })
    }

    /// `G_Damage` on it: the health taken; `Some` once it shatters, with the blow's
    /// `point` and `direction` (`pos1`, `pos2`).
    pub fn hurt(&mut self, damage: i32, point: [f32; 3], direction: [f32; 3]) -> Option<Shatter> {
        if !self.takes_damage || self.shattered {
            return None;
        }
        self.health -= damage;
        if self.health > 0 {
            return None;
        }
        self.shatter(point, direction)
    }

    /// `GlassUse`: shattered from its own middle outwards, away from the middle of the user's
    /// box (the reference takes `other->r.mins + r.maxs` unmoved: for a player, a point near
    /// the world's origin), at 390.
    pub fn used(&mut self, user_box_center: [f32; 3]) -> Option<Shatter> {
        let center: [f32; 3] =
            std::array::from_fn(|axis| (self.bounds.0[axis] + self.bounds.1[axis]) * 0.5);
        let mut direction: [f32; 3] =
            std::array::from_fn(|axis| center[axis] - user_box_center[axis]);
        crate::player_angle_math::normalize(&mut direction);
        self.shatter(center, direction.map(|value| value * 390.0))
    }

    /// `GlassDie`: once only.
    fn shatter(&mut self, point: [f32; 3], direction: [f32; 3]) -> Option<Shatter> {
        if self.shattered {
            return None;
        }
        self.shattered = true;
        Some(Shatter {
            at: self.center(),
            point,
            direction,
            radius: 40,
            shards: self.max_shards,
        })
    }
}
