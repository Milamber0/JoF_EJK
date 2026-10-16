//! Reconstruct cgame's vehicle weapon indices in `CS_MODELS` registration order.
//! The wire carries a vehicle weapon index, not a `CS_EFFECTS` index.

use sjk_game_jka::vehicle_parms::{VehicleCapacity, VehicleFiles, VehicleRegistry, VehicleTable};
use sjk_game_jka::vehicle_presentation::WeaponPresentation;
use sjk_protocol::GameState;
use sjk_vfs::VirtualFileSystem;
use std::sync::Arc;

const CS_MODELS: usize = 298;
const MAX_MODELS: usize = 512;

pub(crate) struct VehicleMissiles {
    files: Arc<VehicleFiles>,
    table: VehicleTable,
    weapons: Vec<WeaponPresentation>,
}

impl VehicleMissiles {
    pub(crate) fn load(vfs: &VirtualFileSystem, game: &GameState) -> Self {
        let files = Arc::new(VehicleFiles::from_listing(
            |directory, extension| vfs.list_files(directory, extension),
            |path| vfs.read(path).ok().flatten().map(|asset| asset.bytes),
        ));
        let mut result = Self {
            table: VehicleTable::new(Arc::clone(&files), VehicleCapacity::REFERENCE),
            files,
            weapons: vec![WeaponPresentation::default()],
        };
        for index in CS_MODELS..CS_MODELS + MAX_MODELS {
            result.refresh(index, game);
        }
        result
    }

    pub(crate) fn refresh(&mut self, index: usize, game: &GameState) {
        if !(CS_MODELS..CS_MODELS + MAX_MODELS).contains(&index) {
            return;
        }
        let Some(name) = game
            .config_string(index)
            .and_then(|name| name.strip_prefix(b"$"))
        else {
            return;
        };
        self.table.index_for_name(name, &mut Unregistered);
        for weapon in &self.table.weapons()[self.weapons.len()..] {
            self.weapons.push(
                weapon
                    .name
                    .as_deref()
                    .map_or_else(WeaponPresentation::default, |name| {
                        self.files.weapon_presentation(name)
                    }),
            );
        }
    }

    pub(crate) fn weapon(&self, index: usize) -> Option<&WeaponPresentation> {
        self.weapons.get(index)
    }

    pub(crate) fn weapons(&self) -> impl Iterator<Item = &WeaponPresentation> {
        self.weapons.iter()
    }

    pub(crate) fn effect_names(&self) -> impl Iterator<Item = &str> {
        self.weapons
            .iter()
            .filter_map(|weapon| weapon.shot_effect.as_deref())
    }
}

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
