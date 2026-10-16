//! The vehicle table: which model and skin a `$<vehicle>` NPC wears.
//!
//! `BG_VehicleLoadParms` (`bg_vehicleLoad.c`) concatenates every `ext_data/vehicles/*.veh`
//! and `BG_VehicleGetIndex` finds a vehicle by its `name` key without regard to case.
//! cgame then loads `models/players/<model>/model.glm` with `model_<skin>.skin`, or
//! `model_default.skin` when the vehicle names no skin (`cg_players.c:8782-8791`).
use sjk_game_jka::vehicle_fields::kind;
use sjk_game_jka::vehicle_parms::{VehicleCapacity, VehicleFiles, VehicleRegistry, VehicleTable};
use sjk_vfs::VirtualFileSystem;
use std::sync::Arc;

/// A vehicle's `type` key, which decides how much of its angles the model root takes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum VehicleKind {
    /// Animals, fliers and untyped vehicles: upright, like any other non-humanoid.
    #[default]
    Upright,
    Speeder,
    Fighter,
    Walker,
}

/// Model directory, skin variant and kind of one vehicle.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VehicleLook {
    pub(crate) directory: String,
    pub(crate) variant: String,
    pub(crate) kind: VehicleKind,
    pub(crate) camera: crate::camera::VehicleProfile,
    pub(crate) hud: crate::hud::vehicle::Profile,
}

/// The look of vehicle `name`, if any mounted `.veh` file defines it. A dozen small text
/// files are read when a vehicle is loaded, which happens when one first appears, never
/// per frame; every actor loading path goes through here, so none needs a table of its own.
pub(crate) fn look(vfs: &VirtualFileSystem, name: &str) -> Option<VehicleLook> {
    let files = VehicleFiles::from_listing(
        |directory, extension| vfs.list_files(directory, extension),
        |path| vfs.read(path).ok().flatten().map(|asset| asset.bytes),
    );
    let mut table = VehicleTable::new(Arc::new(files), VehicleCapacity::REFERENCE);
    let index = table.index_for_name(name.as_bytes(), &mut Unregistered)?;
    let info = table.vehicle(index)?;
    let model = std::str::from_utf8(info.model.as_deref()?).ok()?;
    let skin = info
        .skin
        .as_deref()
        .and_then(|skin| std::str::from_utf8(skin).ok())
        .unwrap_or("");
    let variant = skin
        .split('|')
        .next()
        .filter(|skin| !skin.is_empty())
        .unwrap_or("default");
    Some(VehicleLook {
        directory: format!("models/players/{model}"),
        variant: variant.to_owned(),
        kind: match info.kind {
            kind::SPEEDER => VehicleKind::Speeder,
            kind::FIGHTER => VehicleKind::Fighter,
            kind::WALKER => VehicleKind::Walker,
            _ => VehicleKind::Upright,
        },
        camera: crate::camera::VehicleProfile::from_vehicle(info),
        hud: crate::hud::vehicle::Profile::from_info(info),
    })
}

// Appearance lookup must not register server effects or alter network indices.
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
    fn print(&mut self, _: &str) {}
}
