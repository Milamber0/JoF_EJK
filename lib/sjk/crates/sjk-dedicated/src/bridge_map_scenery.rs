//! What a map places for its clients to draw, on this server: the sky portal
//! (`CS_SKYBOXORG`, and the entities `G_PortalifyEntities` flags for every client), the
//! portal surfaces and their cameras (`locateCamera`), and breakable glass (`func_glass`).
//! The weather entities only name effects, which `bridge_map_effects` registers in the
//! lump's order. The rules are `sjk_game_jka::map_scenery`.

use super::*;
use sjk_game_jka::map_scenery::{
    CS_SKYBOXORG, Camera, ET_PORTAL, EV_GLASS_SHATTER, Glass, PORTALIFY_DELAY, PortalSurface,
    Shatter, camera_roll, sky_portal,
};

/// `s.origin`, `s.origin2`, `s.angles`, `s.frame`, `s.powerups`, `s.clientNum`,
/// `s.eventParm`, `s.genericenemyindex`, `s.trickedentindex`, `s.isPortalEnt`.
const ES_ORIGIN: [usize; 3] = [11, 12, 13];
const ES_ORIGIN2: [usize; 3] = [56, 60, 53];
const ES_ANGLES: [usize; 3] = [25, 9, 24];
const ES_FRAME: usize = 83;
const ES_POWERUPS: usize = 77;
const ES_CLIENT_NUM: usize = 32;
const ES_EVENT_PARM: usize = 42;
const ES_GENERIC_ENEMY: usize = 18;
const ES_TRICKED: usize = 58;
const ES_IS_PORTAL_ENT: usize = 98;
/// `MASK_SHOT`'s solid bit, which the portalify trace uses (`CONTENTS_SOLID`).
const CONTENTS_SOLID: u32 = 1;

/// A portal camera as `locateCamera` reads it: where it is, how it turns and rolls, and
/// the name its own aim is at.
#[derive(Clone, Debug)]
struct PlacedCamera {
    targetname: String,
    target: String,
    origin: [f32; 3],
    spawnflags: u32,
    roll: u32,
}

/// The level's sky portal, portals and glass.
#[derive(Debug, Default)]
pub(in crate::bridge) struct Scenery {
    /// The sky portal's origin and when `G_PortalifyEntities` runs (0 once it has).
    sky: Option<([f32; 3], i32)>,
    portals: Vec<(EntityId, PortalSurface)>,
    cameras: Vec<PlacedCamera>,
    glass: Vec<(EntityId, Glass)>,
}

impl Scenery {
    /// The panes still standing, as obstacles to every trace.
    pub(super) fn obstacles(&self) -> impl Iterator<Item = BoxObstacle> + '_ {
        self.glass.iter().map(|(id, pane)| BoxObstacle {
            entity: id.legacy_number(),
            origin: [0.0; 3],
            bounds: pane.bounds,
            contents: CONTENTS_SOLID,
            model: Some(pane.model),
        })
    }

    /// Lets go of the entities without freeing them: the pool they were in is gone.
    pub(super) fn forget_entities(&mut self) {
        *self = Self::default();
    }
}

impl NativeGame {
    /// `SP_misc_skyportal`, `SP_misc_portal_surface`, `SP_misc_portal_camera` and
    /// `SP_func_glass` for everything the map places in this game type, in lump order.
    pub(super) fn spawn_scenery(&mut self, entities: &[sjk_entity::Entity], level_time: i32) {
        self.stock.scenery.forget_entities();
        for entity in entities.iter().skip(1) {
            if !sjk_game_jka::spawn_table::kept_by_reference(entity, self.gametype) {
                continue;
            }
            let origin = sjk_game_jka::fx_runner::vector(entity, "origin");
            match entity.classname().map(str::to_ascii_lowercase).as_deref() {
                Some("misc_skyportal") => {
                    self.publish_config_string(CS_SKYBOXORG, &sky_portal(entity));
                    self.stock.scenery.sky = Some((origin, level_time + PORTALIFY_DELAY));
                }
                Some("misc_portal_surface") => {
                    let portal = PortalSurface::spawn(entity, level_time);
                    let mut state = EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
                    state.set_raw_field(ES_ENTITY_TYPE, ET_PORTAL);
                    for axis in 0..3 {
                        state.set_raw_field(ES_POS_BASE[axis], origin[axis].to_bits());
                        state.set_raw_field(ES_ORIGIN[axis], origin[axis].to_bits());
                    }
                    project_portal(&mut state, &portal);
                    if let Some(id) = self.pool.spawn_entity(state, level_time) {
                        self.pool.set_bounds(id, ([0.0; 3], [0.0; 3]));
                        self.stock.scenery.portals.push((id, portal));
                    }
                }
                Some("misc_portal_camera") => {
                    let camera = PlacedCamera {
                        targetname: entity.get("targetname").unwrap_or_default().to_owned(),
                        target: entity.get("target").unwrap_or_default().to_owned(),
                        origin,
                        spawnflags: entity
                            .get("spawnflags")
                            .map_or(0, |value| sjk_game_jka::userinfo::atoi(value.as_bytes()))
                            as u32,
                        roll: camera_roll(entity),
                    };
                    // Linked with no model: sent, drawn as nothing.
                    let mut state = EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
                    for axis in 0..3 {
                        state.set_raw_field(ES_POS_BASE[axis], origin[axis].to_bits());
                        state.set_raw_field(ES_ORIGIN[axis], origin[axis].to_bits());
                    }
                    state.set_raw_field(ES_CLIENT_NUM, camera.roll);
                    if let Some(id) = self.pool.spawn_entity(state, level_time) {
                        self.pool.set_bounds(id, ([0.0; 3], [0.0; 3]));
                    }
                    self.stock.scenery.cameras.push(camera);
                }
                Some("func_glass") => {
                    let Some(map) = self.map.as_ref() else {
                        continue;
                    };
                    let model = entity
                        .get("model")
                        .and_then(|model| model.strip_prefix('*'))
                        .and_then(|number| number.parse().ok());
                    let Some(bounds) =
                        model.and_then(|model| crate::map::brush_bounds(&map.bsp, model))
                    else {
                        continue;
                    };
                    let Some(pane) = Glass::spawn(entity, bounds) else {
                        continue;
                    };
                    let mut state = EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
                    state.set_raw_field(ES_ENTITY_TYPE, sjk_game_jka::movers::ET_MOVER);
                    sjk_game_jka::triggers::set_brush_model(&mut state, pane.model);
                    if let Some(id) = self.pool.spawn_entity(state, level_time) {
                        self.pool.set_bounds(id, pane.bounds);
                        self.stock.scenery.glass.push((id, pane));
                    }
                }
                _ => {}
            }
        }
    }

    /// The thinks of the scenery due at `level_time`: `locateCamera`, `G_PortalifyEntities`.
    pub(super) fn run_scenery(&mut self, level_time: i32) {
        for index in 0..self.stock.scenery.portals.len() {
            let portal = &self.stock.scenery.portals[index].1;
            if portal.locate_at == 0 || portal.locate_at > level_time {
                continue;
            }
            let target = portal.target.clone();
            let camera = self.pick_camera(&target);
            let (id, portal) = &mut self.stock.scenery.portals[index];
            let id = *id;
            if portal.locate(camera) {
                if let Some(state) = self.pool.state_mut(id) {
                    project_portal(state, portal);
                }
            } else {
                println!("Couldn't find target for misc_partal_surface");
                self.pool.free(id, level_time);
            }
        }
        if let Some((origin, at)) = self.stock.scenery.sky
            && at != 0
            && at <= level_time
        {
            self.portalify(origin);
            self.stock.scenery.sky = Some((origin, 0));
        }
    }

    /// `G_PickTarget(target)` for a portal surface (C `rand() % count` among everything
    /// so named), read as a camera; and the camera's own aim, picked the same way.
    fn pick_camera(&mut self, target: &str) -> Option<Camera> {
        let named: Vec<(String, [f32; 3], [f32; 3])> = self
            .stock
            .named
            .iter()
            .filter(|named| named.name.eq_ignore_ascii_case(target))
            .map(|named| (named.classname.clone(), named.origin, named.angles))
            .collect();
        if named.is_empty() {
            return None;
        }
        let (classname, origin, angles) = named[self.rand.next() as usize % named.len()].clone();
        let placed = (classname == "misc_portal_camera")
            .then(|| {
                self.stock
                    .scenery
                    .cameras
                    .iter()
                    .find(|camera| {
                        camera.targetname.eq_ignore_ascii_case(target) && camera.origin == origin
                    })
                    .cloned()
            })
            .flatten();
        let (spawnflags, roll, aim_name) = placed.map_or((0, 0, String::new()), |camera| {
            (camera.spawnflags, camera.roll, camera.target)
        });
        let aims: Vec<[f32; 3]> = if aim_name.is_empty() {
            Vec::new()
        } else {
            self.stock
                .named
                .iter()
                .filter(|named| named.name.eq_ignore_ascii_case(&aim_name))
                .map(|named| named.origin)
                .collect()
        };
        let aim = (!aims.is_empty()).then(|| aims[self.rand.next() as usize % aims.len()]);
        Some(Camera {
            origin,
            angles,
            spawnflags,
            roll,
            aim,
        })
    }

    /// `G_PortalifyEntities`: every entity but a player's that the sky portal's origin sees
    /// (in its PVS, a clear line through the world) is a portal entity, sent to everyone.
    fn portalify(&mut self, origin: [f32; 3]) {
        let Some(map) = self.map.as_ref() else { return };
        let eye = crate::visibility::Eye::new(&map.bsp, &map.areas, origin);
        let world = WorldCollision {
            bsp: &map.bsp,
            scratch: &map.scratch,
        };
        let flagged: Vec<EntityId> = self
            .pool
            .entities()
            .filter(|(_, linked, ..)| *linked)
            .filter_map(|(id, _, state, _)| {
                let at = std::array::from_fn(|axis| {
                    f32::from_bits(state.raw_field(ES_POS_BASE[axis]).unwrap_or(0))
                });
                let seen = eye.sees_point(&map.bsp, at)
                    && world
                        .trace(origin, [0.0; 3], [0.0; 3], at, CONTENTS_SOLID)
                        .fraction
                        == 1.0;
                seen.then_some(id)
            })
            .collect();
        for id in flagged {
            if let Some(state) = self.pool.state_mut(id) {
                state.set_raw_field(ES_IS_PORTAL_ENT, 1);
            }
            self.pool.set_broadcast(id, true);
        }
    }

    /// `G_Damage` on a pane of glass by `owner` (`number` its entity): `false` when `number`
    /// is no pane. The blow's point and direction are not known here: the pane's middle and
    /// the way from the attacker to it stand in for them.
    pub(in crate::bridge) fn hurt_glass(
        &mut self,
        number: u16,
        damage: i32,
        owner: u16,
        level_time: i32,
    ) -> bool {
        let Some(index) = self
            .stock
            .scenery
            .glass
            .iter()
            .position(|(id, _)| id.legacy_number() == number)
        else {
            return false;
        };
        let from = self
            .peer(usize::from(owner))
            .map(|peer| peer.state.origin());
        let pane = &mut self.stock.scenery.glass[index].1;
        let center = pane.center();
        let mut direction = from.map_or([0.0; 3], |from| {
            std::array::from_fn(|axis| center[axis] - from[axis])
        });
        sjk_game_jka::player_angle_math::normalize(&mut direction);
        if let Some(shatter) = pane.hurt(damage, center, direction) {
            let attacker = (usize::from(owner) < self.players.places())
                .then_some(usize::from(owner))
                .unwrap_or(usize::MAX);
            self.shatter_glass(index, shatter, attacker, level_time);
        }
        true
    }

    /// `GlassUse` for the panes called `name`, by `client` (its box's middle unmoved, as the
    /// reference takes it; the world's origin for no client).
    pub(super) fn use_glass(&mut self, name: &str, client: usize, level_time: i32) {
        let mut index = 0;
        while index < self.stock.scenery.glass.len() {
            let pane = &mut self.stock.scenery.glass[index].1;
            if !pane.targetname.eq_ignore_ascii_case(name) {
                index += 1;
                continue;
            }
            let user = if client == usize::MAX {
                [0.0; 3]
            } else {
                [0.0, 0.0, 8.0]
            };
            match pane.used(user) {
                Some(shatter) => self.shatter_glass(index, shatter, client, level_time),
                None => index += 1,
            }
        }
    }

    /// `GlassDie`: its targets used for the attacker, the shatter event, the pane freed.
    fn shatter_glass(&mut self, index: usize, shatter: Shatter, attacker: usize, level_time: i32) {
        let (id, pane) = self.stock.scenery.glass.remove(index);
        self.fire_targets(&pane.target, attacker, level_time);
        let mut event = EventEntity {
            event: EV_GLASS_SHATTER,
            parameter: 0,
            origin: shatter.at,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        event.extra[0] = (ES_GENERIC_ENEMY, u32::from(id.legacy_number()));
        for axis in 0..3 {
            event.extra[1 + axis] = (ES_ORIGIN[axis], shatter.point[axis].to_bits());
            event.extra[4 + axis] = (ES_ANGLES[axis], shatter.direction[axis].to_bits());
        }
        event.extra[7] = (ES_TRICKED, shatter.radius);
        event.extra[8] = (ES_POS_TIME, shatter.shards as u32);
        let _ = self.pool.spawn_temporary(event.state(), level_time, None);
        self.pool.free(id, level_time);
    }
}

/// A portal surface's wire fields.
fn project_portal(state: &mut EntityState, portal: &PortalSurface) {
    for axis in 0..3 {
        state.set_raw_field(ES_ORIGIN2[axis], portal.origin2[axis].to_bits());
    }
    state.set_raw_field(ES_FRAME, portal.frame);
    state.set_raw_field(ES_POWERUPS, portal.powerups);
    state.set_raw_field(ES_CLIENT_NUM, portal.client_num);
    state.set_raw_field(ES_EVENT_PARM, portal.event_parm);
}
