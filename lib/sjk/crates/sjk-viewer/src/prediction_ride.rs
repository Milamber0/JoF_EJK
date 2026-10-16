//! The local player's ride in its prediction: `CG_PredictPlayerState`'s vehicle branch
//! (`cg_predict.c:1283-1386`), through [`sjk_game_jka::vehicle_predict`].
//!
//! While the local player pilots a vehicle (`CG_Piloting`: the vehicle entity's `owner` is
//! the local client) and the snapshot carries the vehicle's player state (`vps`), every
//! predicted command runs the pilot's move and then the vehicle's by the same command; the
//! pilot's predicted origin is the predicted vehicle's (its driver tag). The vehicle's
//! definition comes from the client's own `ext_data/vehicles` by the name its model
//! configstring gives it (`$swoop_mp`), as the cgame's `BG_VehicleGetIndex` finds it.
//!
//! The pilot sits on the vehicle model's `*driver` tag (`AttachRidersGeneric`): the client
//! poses the model as the server does ([`sjk_game_jka::vehicle_skeleton`]) — the root on
//! the vehicle's legs animation, installed when a snapshot shows it change, read at the
//! snapshot's time for replay. Presentation resamples that bolt at cgame time,
//! as `AttachRidersGeneric` does, so the camera does not hold a network-rate pose.
//!
//! Presentation reads this same predicted vehicle root for its model and rider seat
//! (`cg_ents.c`, `CG_AddPacketEntities` and `CG_CalcEntityLerpPositions`).

use std::sync::Arc;

use sjk_client::pmove::{MovementCollision, MovementConfig, MovementState, Predictor};
use sjk_game_jka::vehicle::Vehicle;
use sjk_game_jka::vehicle_parms::{VehicleCapacity, VehicleFiles, VehicleRegistry, VehicleTable};
use sjk_game_jka::vehicle_predict::RidePrediction;
use sjk_game_jka::vehicle_skeleton::{VehicleModels, VehicleSkeleton};
use sjk_protocol::{GameState, Snapshot, UserCommand};
use sjk_vfs::VirtualFileSystem;

/// `CS_MODELS`, `CS_SYSTEMINFO`.
const CS_MODELS: usize = 298;
const CS_SYSTEMINFO: usize = 1;
/// The vehicle entity's `s.legsAnim`, `s.legsFlip`: every client is sent them.
const ES_LEGS_ANIM: usize = 16;
const ES_LEGS_FLIP: usize = 62;

/// The rides this client can predict and the one it predicts now.
pub(crate) struct Rides {
    table: VehicleTable,
    /// Vehicle definitions met, by name: a fresh vehicle of each (`None` for a name the
    /// table does not know).
    templates: Vec<(Vec<u8>, Option<Vehicle>)>,
    /// The ride the newest snapshot shows: the template's index and the vehicle's `solid`.
    piloting: Option<(usize, u32)>,
    /// `bg_fighterAltControl` from the server's system info: free pitch and roll in a
    /// fighter.
    fighter_alt_control: bool,
    /// The ride replayed from the newest snapshot, and the frame's preview of it.
    pub(super) committed: Option<RidePrediction>,
    pub(super) preview: Option<RidePrediction>,
    /// The client's files, for the vehicles' models; the models read, by name (`None` for
    /// one that will not load); and the ridden vehicle's skeleton, by its number.
    files: VirtualFileSystem,
    models: Vec<(Vec<u8>, Option<Arc<VehicleModels>>)>,
    skeleton: Option<(u16, Arc<VehicleModels>, VehicleSkeleton)>,
}

impl Rides {
    /// Read the animated seat at cgame time for presentation only. Snapshot-time
    /// offsets remain in command replay; gait bob must not become prediction error.
    pub(super) fn present_pilot(&mut self, pilot: &mut MovementState, time: i32) {
        let Some(ride) = self.preview.as_ref().or(self.committed.as_ref()) else {
            return;
        };
        if pilot.vehicle_entity_num != ride.number() {
            return;
        }
        let Some((number, models, skeleton)) = self.skeleton.as_mut() else {
            return;
        };
        if *number != ride.number() {
            return;
        }
        if let Ok(Some(offset)) = skeleton.offset(models, models.bolt("*driver"), time) {
            pilot.origin = sjk_game_jka::vehicle_riders::driver_origin(
                ride.state().origin,
                ride.state().view_angles[1],
                offset,
            );
        }
    }

    /// The client's vehicle definitions, read from `vfs` (`BG_VehicleLoadParms`).
    pub(crate) fn new(vfs: &VirtualFileSystem) -> Self {
        let files = VehicleFiles::from_listing(
            |directory, extension| vfs.list_files(directory, extension),
            |path| {
                vfs.read(path)
                    .ok()
                    .flatten()
                    .map(|asset| asset.bytes.to_vec())
            },
        );
        Self {
            table: VehicleTable::new(Arc::new(files), VehicleCapacity::REFERENCE),
            templates: Vec::new(),
            piloting: None,
            fighter_alt_control: false,
            committed: None,
            preview: None,
            files: vfs.clone(),
            models: Vec::new(),
            skeleton: None,
        }
    }

    /// Which vehicle, if any, the local player pilots in `snapshot` (`CG_Piloting`), and
    /// its definition from its model's configstring in `game_state`.
    pub(crate) fn resolve(&mut self, snapshot: &Snapshot, game_state: &GameState) {
        self.piloting = None;
        // `CS_SYSTEMINFO` carries `bg_fighterAltControl` (`CVAR_SYSTEMINFO`).
        self.fighter_alt_control = game_state
            .config_string(CS_SYSTEMINFO)
            .and_then(|info| sjk_protocol::info_value(info, b"bg_fighterAltControl"))
            .is_some_and(|value| {
                std::str::from_utf8(value)
                    .ok()
                    .and_then(|value| value.trim().parse::<i32>().ok())
                    .unwrap_or(0)
                    != 0
            });
        let number = snapshot.player.vehicle_entity_num();
        if number == 0 || snapshot.vehicle_player.is_none() {
            return;
        }
        let Some(vehicle) = snapshot
            .entities
            .iter()
            .find(|entity| entity.number() == number)
        else {
            return;
        };
        if vehicle.owner() != snapshot.player.client_num() {
            return;
        }
        let model = vehicle.raw_field(46).unwrap_or(0) as usize;
        let Some(name) = game_state
            .config_string(CS_MODELS + model)
            .and_then(|name| name.strip_prefix(b"$"))
        else {
            return;
        };
        let index = match self
            .templates
            .iter()
            .position(|(known, _)| known.as_slice() == name)
        {
            Some(index) => index,
            None => {
                let template =
                    self.table
                        .index_for_name(name, &mut Unregistered)
                        .and_then(|index| {
                            Some(Vehicle::new(
                                Arc::new(self.table.vehicle(index)?.clone()),
                                index,
                            ))
                        });
                self.templates.push((name.to_vec(), template));
                self.templates.len() - 1
            }
        };
        if self.templates[index].1.is_some() {
            self.piloting = Some((index, vehicle.solid()));
        }
    }

    /// The committed ride seeded from `snapshot` (in the storage it has), or none where
    /// the local player pilots nothing the client can predict.
    pub(super) fn seed(&mut self, snapshot: &Snapshot, command_time: i32, config: MovementConfig) {
        self.preview = None;
        let (Some((index, solid)), Some(vehicle_state)) =
            (self.piloting, snapshot.vehicle_player.as_ref())
        else {
            self.committed = None;
            return;
        };
        let Some(template) = self.templates[index].1.as_ref() else {
            return;
        };
        let pilot = snapshot.player.client_num();
        let model = template.info.model.clone();
        match &mut self.committed {
            Some(ride) => ride.reseed(vehicle_state, template, solid, pilot, command_time, config),
            None => {
                self.committed = Some(RidePrediction::new(
                    vehicle_state,
                    template,
                    solid,
                    pilot,
                    command_time,
                    config,
                ))
            }
        }
        let number = vehicle_state.client_num();
        let legs = snapshot
            .entities
            .iter()
            .find(|entity| entity.number() == number)
            .map_or((0, false), |entity| {
                (
                    entity.raw_field(ES_LEGS_ANIM).unwrap_or(0) as u16,
                    entity.raw_field(ES_LEGS_FLIP).unwrap_or(0) != 0,
                )
            });
        let offset = self.driver_offset(model.as_deref(), number, legs, snapshot.server_time);
        if let Some(ride) = self.committed.as_mut() {
            ride.set_driver_offset(offset);
            ride.set_fighter_alt_control(self.fighter_alt_control);
            // The point a ship boundary turns it toward, as the snapshot has it.
            let index = vehicle_state.vehicle_fields().turnaround_index;
            let target = snapshot
                .entities
                .iter()
                .find(|entity| i32::from(entity.number()) == index && index != 0);
            ride.set_turnaround_target(target.map(|entity| {
                [11, 12, 13].map(|field| f32::from_bits(entity.raw_field(field).unwrap_or(0)))
            }));
        }
    }

    /// Where `*driver` is on the ridden vehicle's model as the server poses it
    /// (`G_UpdateClientAnims` at the snapshot's frame): zero where the client has no model.
    fn driver_offset(
        &mut self,
        model: Option<&[u8]>,
        number: u16,
        (legs, flip): (u16, bool),
        time: i32,
    ) -> [f32; 3] {
        let Some(models) = model.and_then(|model| self.models_of(model)) else {
            return [0.0; 3];
        };
        if !self
            .skeleton
            .as_ref()
            .is_some_and(|(known, held, _)| *known == number && Arc::ptr_eq(held, &models))
        {
            self.skeleton = Some((number, Arc::clone(&models), VehicleSkeleton::new(&models)));
        }
        let Some((_, models, skeleton)) = self.skeleton.as_mut() else {
            return [0.0; 3];
        };
        if skeleton
            .update_animation(models, legs, flip, 1.0, time)
            .is_err()
        {
            return [0.0; 3];
        }
        skeleton
            .offset(models, models.bolt("*driver"), time)
            .ok()
            .flatten()
            .unwrap_or([0.0; 3])
    }

    /// `models/players/<model>/model.glm` with its skeleton and animations, read once.
    fn models_of(&mut self, model: &[u8]) -> Option<Arc<VehicleModels>> {
        if let Some((_, known)) = self
            .models
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(model))
        {
            return known.clone();
        }
        let read = |path: &str| {
            self.files
                .read(path)
                .ok()
                .flatten()
                .map(|asset| asset.bytes)
        };
        let loaded = (|| {
            let glm = read(&format!(
                "models/players/{}/model.glm",
                String::from_utf8_lossy(model)
            ))?;
            let skeleton = VehicleModels::skeleton_name(&glm)?;
            let directory = skeleton
                .rsplit_once('/')
                .map_or(skeleton.as_str(), |(directory, _)| directory);
            VehicleModels::new(
                &glm,
                &read(&format!("{skeleton}.gla"))?,
                &read(&format!("{directory}/animation.cfg"))?,
            )
            .ok()
            .map(Arc::new)
        })();
        self.models.push((model.to_vec(), loaded.clone()));
        loaded
    }

    /// The frame's preview ride copied from the committed one, in the storage it has.
    pub(super) fn begin_preview(&mut self) -> Option<&mut RidePrediction> {
        let committed = self.committed.as_ref()?;
        match &mut self.preview {
            Some(preview) => preview.clone_from(committed),
            None => self.preview = Some(committed.clone()),
        }
        self.preview.as_mut()
    }
}

/// CG_PredictPlayerState measures the vehicle while piloting, the player on
/// foot, and neither for a passenger. An animated driver bolt is not a miss.
pub(super) fn error_state<'a>(
    pilot: &'a MovementState,
    ride: Option<&'a RidePrediction>,
) -> Option<&'a MovementState> {
    if pilot.vehicle_entity_num == 0 {
        Some(pilot)
    } else {
        ride.filter(|ride| ride.number() == pilot.vehicle_entity_num)
            .map(RidePrediction::state)
    }
}

/// One predicted command: the ride's two moves where the player pilots one, its own move
/// otherwise.
pub(super) fn predict(
    predictor: &mut Predictor,
    ride: Option<&mut RidePrediction>,
    command: UserCommand,
    collision: &impl MovementCollision,
) {
    match ride {
        Some(ride) => ride.predict(predictor, command, collision),
        None => predictor.predict_command(command, collision),
    }
}

/// The cgame registers nothing when it reads a vehicle for its prediction.
struct Unregistered;

impl VehicleRegistry for Unregistered {
    fn model_index(&mut self, _: &[u8]) -> i32 {
        0
    }
    fn sound_index(&mut self, _: &[u8]) -> i32 {
        0
    }
    fn effect_index(&mut self, _: &[u8]) -> i32 {
        0
    }
    fn print(&mut self, _: &str) {
        // A definition the client cannot read leaves the ride unpredicted, as the snapshots
        // show it.
    }
}
