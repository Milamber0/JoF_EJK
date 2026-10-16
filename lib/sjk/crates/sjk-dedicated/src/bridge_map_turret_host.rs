//! The server as a map turret asks for it ([`TurretHost`]): the level's tables, the map's
//! traces and PVS, the turret's Ghoul2 instance and bolts, and the entities and shots it
//! makes. What a turret asks to be done to the level beyond itself — names used, splashes
//! — is kept for [`NativeGame::run_map_turrets`] to carry out once the turret's call is
//! over.

use super::*;
use sjk_game_jka::map_turret_model::{TurretModel, TurretPose};
use sjk_game_jka::map_turret_world::{BoltIndex, Shot, TurretHost};
use sjk_game_jka::registries::{BoneTable, EffectTable};
use std::sync::Arc;

/// A splash a turret asked for (`G_RadiusDamage`).
#[derive(Clone, Copy, Debug)]
pub(super) struct Blast {
    pub(super) origin: [f32; 3],
    pub(super) attacker: Option<u16>,
    pub(super) damage: f32,
    pub(super) radius: f32,
    pub(super) ignore: Option<u16>,
    pub(super) means: u32,
}

/// A turret's Ghoul2 instance on the server: its model and its pose.
pub(super) struct Instance {
    pub(super) number: u16,
    pub(super) model: Arc<TurretModel>,
    pub(super) pose: TurretPose,
}

/// What a turret's call asked for beyond itself, and the server's Ghoul2 of the turrets.
#[derive(Default)]
pub(super) struct TurretWork {
    /// Names used (`G_UseTargets2`), with the activator.
    pub(super) uses: Vec<(String, Option<u16>)>,
    pub(super) blasts: Vec<Blast>,
    /// Turrets whose model went (`G_KillG2Queue`).
    pub(super) killed: Vec<u16>,
    /// Items registered (`RegisterItem`), by item-list row.
    pub(super) items: Vec<usize>,
    /// Each model's files by path, loaded once (`None`: the map's files lack it).
    pub(super) models: Vec<(&'static str, Option<Arc<TurretModel>>)>,
    pub(super) instances: Vec<Instance>,
    /// The bolt names `G2API_AddBolt` handed out, by handle.
    pub(super) bolts: Vec<String>,
    /// `G_IconIndex`'s table (`CS_ICONS`).
    pub(super) icons: Vec<Vec<u8>>,
}

/// `CS_ICONS`, `MAX_ICONS`.
const CS_ICONS: usize = 1_067;
const MAX_ICONS: usize = 64;

/// The server's side of a turret's call.
pub(super) struct ServerTurretHost<'a> {
    pub(super) rng: &'a mut Rng,
    pub(super) models: &'a mut ModelTable,
    pub(super) sounds: &'a mut SoundTable,
    pub(super) effects: &'a mut EffectTable,
    pub(super) bones: &'a mut BoneTable,
    /// The configstrings registered, for the clients.
    pub(super) registered: &'a mut Vec<(usize, Vec<u8>)>,
    pub(super) map: Option<&'a LoadedMap>,
    /// The level's boxes but the turret's own and what it owns ([`NativeGame::gather_obstacles`]).
    pub(super) obstacles: &'a [BoxObstacle],
    pub(super) pool: &'a mut EntityPool,
    pub(super) missiles: &'a mut Vec<(EntityId, Missile)>,
    pub(super) work: &'a mut TurretWork,
    pub(super) level_time: i32,
    /// The Ghoul2 clock: the frame before (`SV_Frame` sets it after the game's frame).
    pub(super) clock: i32,
}

impl ServerTurretHost<'_> {
    /// Model `path`'s files, loaded the first time it is asked for.
    fn model(&mut self, path: &'static str) -> Option<Arc<TurretModel>> {
        if let Some((_, model)) = self.work.models.iter().find(|(known, _)| *known == path) {
            return model.clone();
        }
        let map = self.map?;
        let read = |path: &str| map.files.read(path).ok().flatten().map(|asset| asset.bytes);
        let loaded = read(path).and_then(|glm| {
            let skeleton = sjk_game_jka::vehicle_skeleton::VehicleModels::skeleton_name(&glm)?;
            let gla = read(&format!("{skeleton}.gla"))?;
            TurretModel::new(&glm, &gla).ok().map(Arc::new)
        });
        if loaded.is_none() {
            eprintln!("misc_turretG2: no server model {path}; its shots leave from its origin");
        }
        self.work.models.push((path, loaded.clone()));
        loaded
    }

    fn instance(&mut self, me: u16) -> Option<&mut Instance> {
        self.work
            .instances
            .iter_mut()
            .find(|instance| instance.number == me)
    }

    fn register(&mut self) -> impl FnMut(usize, &[u8]) + '_ {
        |index, value| self.registered.push((index, value.to_vec()))
    }
}

impl TurretHost for ServerTurretHost<'_> {
    fn rng(&mut self) -> &mut Rng {
        self.rng
    }

    fn model_index(&mut self, name: &[u8]) -> u16 {
        let (models, registered) = (&mut *self.models, &mut *self.registered);
        models.index(name, &mut |index, value| {
            registered.push((index, value.to_vec()))
        })
    }

    fn sound_index(&mut self, name: &[u8]) -> u16 {
        let (sounds, registered) = (&mut *self.sounds, &mut *self.registered);
        sounds.index(name, &mut |index, value| {
            registered.push((index, value.to_vec()))
        })
    }

    fn effect_index(&mut self, name: &[u8]) -> u16 {
        let (effects, registered) = (&mut *self.effects, &mut *self.registered);
        effects.index(name, &mut |index, value| {
            registered.push((index, value.to_vec()))
        })
    }

    fn bone_index(&mut self, name: &[u8]) -> u16 {
        let (bones, registered) = (&mut *self.bones, &mut *self.registered);
        bones.index(name, &mut |index, value| {
            registered.push((index, value.to_vec()))
        })
    }

    /// `G_IconIndex` (`CS_ICONS`): what the clients' radar draws a turret by.
    fn icon_index(&mut self, name: &[u8]) -> u16 {
        if let Some(index) = self
            .work
            .icons
            .iter()
            .position(|known| known.eq_ignore_ascii_case(name))
        {
            return index as u16 + 1;
        }
        if self.work.icons.len() + 1 >= MAX_ICONS {
            eprintln!(
                "G_IconIndex: overflow; {} not registered",
                String::from_utf8_lossy(name)
            );
            return 0;
        }
        self.work.icons.push(name.to_vec());
        let index = self.work.icons.len();
        (self.register())(CS_ICONS + index, name);
        index as u16
    }

    fn register_weapon(&mut self, weapon: u32) {
        if let Some(item) = sjk_game_jka::dropped_items::item_for_weapon(weapon) {
            self.work.items.push(item);
        }
    }

    fn trace(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        _: u16,
        mask: u32,
    ) -> sjk_game_jka::pmove::MovementTrace {
        let players = self.obstacles;
        match self.map {
            Some(map) => WithPlayers {
                world: WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                },
                players,
            }
            .trace(start, [0.0; 3], [0.0; 3], end, mask),
            None => WithPlayers {
                world: Void,
                players,
            }
            .trace(start, [0.0; 3], [0.0; 3], end, mask),
        }
    }

    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool {
        self.map
            .is_none_or(|map| Eye::new(&map.bsp, &map.areas, from).sees_point(&map.bsp, to))
    }

    fn point_contents(&mut self, point: [f32; 3], _: u16) -> u32 {
        self.map.map_or(0, |map| {
            WorldCollision {
                bsp: &map.bsp,
                scratch: &map.scratch,
            }
            .point_contents(point)
        })
    }

    fn init_model(&mut self, me: u16, model: &[u8]) {
        self.work.instances.retain(|instance| instance.number != me);
        let path = if model.ends_with(b"laser_cannon_model.glm") {
            "models/map_objects/wedge/laser_cannon_model.glm"
        } else {
            "models/map_objects/imp_mine/turret_canon.glm"
        };
        if let Some(model) = self.model(path) {
            let pose = TurretPose::new(&model);
            self.work.instances.push(Instance {
                number: me,
                model,
                pose,
            });
        }
    }

    fn remove_model(&mut self, me: u16) {
        self.work.instances.retain(|instance| instance.number != me);
        self.work.killed.push(me);
    }

    fn add_bolt(&mut self, _: u16, name: &[u8]) -> BoltIndex {
        let name = String::from_utf8_lossy(name);
        match self.work.bolts.iter().position(|known| *known == name) {
            Some(index) => index as BoltIndex,
            None => {
                self.work.bolts.push(name.into_owned());
                self.work.bolts.len() as BoltIndex - 1
            }
        }
    }

    /// The bolt on the turret's posed model; where the model or the bolt is missing, its
    /// origin, unturned (what the engine answers for a bolt it cannot find).
    fn bolt_matrix(
        &mut self,
        me: u16,
        bolt: BoltIndex,
        angles: [f32; 3],
        origin: [f32; 3],
        scale: [f32; 3],
    ) -> [[f32; 4]; 3] {
        let name = usize::try_from(bolt)
            .ok()
            .and_then(|bolt| self.work.bolts.get(bolt))
            .cloned();
        let clock = self.clock;
        let posed = name.zip(self.instance(me)).and_then(|(name, instance)| {
            instance
                .pose
                .bolt_matrix(&instance.model, &name, angles, origin, scale, clock)
                .ok()
                .flatten()
        });
        posed.unwrap_or_else(|| {
            std::array::from_fn(|row| {
                std::array::from_fn(|column| {
                    if column == 3 {
                        origin[row]
                    } else if column == row {
                        1.0
                    } else {
                        0.0
                    }
                })
            })
        })
    }

    fn set_bone_angles(&mut self, me: u16, bone: &[u8], angles: [f32; 3]) {
        let time = self.level_time;
        if let Some(instance) = self.instance(me) {
            instance.pose.set_bone_angles(
                &instance.model,
                &String::from_utf8_lossy(bone),
                angles,
                time,
            );
        }
    }

    /// The turbolaser's firing and resting animations are the clients' to play: the server
    /// does not pose its barrels by them (its muzzles stay where its pitch puts them).
    fn set_bone_anim(&mut self, _: u16, _: i32, _: i32) {}

    fn raise(&mut self, event: EventEntity) {
        let _ = self
            .pool
            .spawn_temporary(event.state(), self.level_time, None);
    }

    fn launch(&mut self, shot: Shot) {
        if let Some(id) = self
            .pool
            .spawn_entity(shot.missile.state.clone(), self.level_time)
        {
            self.pool.set_bounds(id, shot.missile.bounds);
            self.missiles.push((id, shot.missile));
        }
    }

    fn use_targets(&mut self, name: &str, _: u16, activator: Option<u16>) {
        self.work.uses.push((name.to_owned(), activator));
    }

    fn radius_damage(
        &mut self,
        origin: [f32; 3],
        attacker: Option<u16>,
        damage: f32,
        radius: f32,
        ignore: Option<u16>,
        means: u32,
    ) {
        self.work.blasts.push(Blast {
            origin,
            attacker,
            damage,
            radius,
            ignore,
            means,
        });
    }
}
