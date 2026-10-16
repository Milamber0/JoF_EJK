//! Vehicle definitions (`codemp/game/bg_vehicleLoad.c`): what `ext_data/vehicles/*.veh`
//! and `ext_data/vehicles/weapons/*.vwp` say a vehicle and a vehicle weapon are, read the
//! way the game reads them.
//!
//! - [`VehicleFiles`] is `VehicleParms` and `VehWeaponParms`: every file joined in the
//!   listing's order, uncompressed, a space put between a file that ends on `}` and the
//!   next (`BG_VehicleLoadParms`, `BG_VehWeaponLoadParms`, `bg_vehicleLoad.c:1239-1383`).
//! - [`VehicleTable`] is `g_vehicleInfo` and `g_vehWeaponInfo` with their counts: a
//!   vehicle is read the first time its name is asked for (`BG_VehicleGetIndex`,
//!   `VEH_VehicleIndexForName`, `VEH_LoadVehicle`), and the weapons it names with it
//!   (`VEH_VehWeaponIndexForName`, `VEH_LoadVehWeapon`). Entry 0 of each is the empty
//!   default.
//!
//! What is registered goes through a [`VehicleRegistry`] in the reference's order: the
//! sounds and effects its keys name as they are read, a weapon's model, then the
//! vehicle's model and the effects and sounds every vehicle registers.
//!
//! **Capacity.** The reference holds 16 vehicles and 16 weapons (`MAX_VEHICLES`,
//! `MAX_VEH_WEAPONS`) and refuses the 17th. Here the capacity is the table's
//! ([`VehicleCapacity`]); [`VehicleCapacity::REFERENCE`] is the reference's, which a
//! server keeps for the same maps to behave the same.
//!
//! Held to `tools/game-oracle/vehparms.c` (`game-vehparms.txt`).

use crate::text_parse::{TextParser, atof, until_nul};
use crate::vehicle_fields::{
    Field, Slot, VEHICLE_FIELDS, VehicleInfo, VehicleWeaponInfo, WEAPON_FIELDS, find, kind,
};

/// `MAX_VEHICLE_DATA_SIZE`, `MAX_VEH_WEAPON_DATA_SIZE`.
const MAX_VEHICLE_DATA_SIZE: usize = 0x10_0000;
const MAX_WEAPON_DATA_SIZE: usize = 0x4_0000;
/// The name buffers of `VEH_LoadVehicle` (`char parmName[128]`, `weap1[128]`, ...).
const NAME_BUFFER: usize = 128;
/// `BG_Parse*Parm`'s `char value[1024]`.
const VALUE_BUFFER: usize = 1024;
/// `MAX_VEHICLE_MUZZLES` of which the files name ten (`weapMuzzle1` to `weapMuzzle10`).
const NAMED_MUZZLES: usize = 10;

/// Why the files could not be joined, where the game stops the map (`ERR_DROP`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleFilesTooLarge {
    /// `Vehicle extensions (*.veh) are too large`.
    Vehicles { file: String },
    /// `Vehicle Weapon extensions (*.vwp) are too large`.
    Weapons { file: String },
}

impl std::fmt::Display for VehicleFilesTooLarge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Vehicles { file } => {
                write!(f, "Vehicle extensions (*.veh) are too large (at {file})")
            }
            Self::Weapons { file } => write!(
                f,
                "Vehicle Weapon extensions (*.vwp) are too large (at {file})"
            ),
        }
    }
}

impl std::error::Error for VehicleFilesTooLarge {}

/// `VehicleParms` and `VehWeaponParms`: the definition files joined.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VehicleFiles {
    vehicles: Vec<u8>,
    weapons: Vec<u8>,
}

/// The joining of `BG_VehicleLoadParms`: each file up to its first NUL, a space before it
/// when the text so far ends on `}`; a file that would take the text to `limit` stops.
fn join<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    limit: usize,
) -> Result<Vec<u8>, String> {
    let mut text = Vec::new();
    for (name, contents) in files {
        if text.last() == Some(&b'}') {
            text.push(b' ');
        }
        if text.len() + contents.len() >= limit {
            return Err(name.to_owned());
        }
        text.extend_from_slice(until_nul(contents));
    }
    Ok(text)
}

impl VehicleFiles {
    /// Read the client-only assets without changing server registration order.
    pub fn weapon_presentation(
        &self,
        name: &[u8],
    ) -> crate::vehicle_presentation::WeaponPresentation {
        crate::vehicle_presentation::parse(&self.weapons, name)
    }

    /// `BG_VehicleLoadParms` and `BG_VehWeaponLoadParms` (`bg_vehicleLoad.c:1239-1383`):
    /// every `ext_data/vehicles/*.veh` and `ext_data/vehicles/weapons/*.vwp` the game's
    /// listings hold, in their order — as the server's game and a client's cgame both load
    /// them. Definitions that outgrow the game's buffers stop the reference's map; here the
    /// vehicles are left undefined (every vehicle spawn refused, no ride predicted).
    ///
    /// `list` names the files of a directory with an extension (`FS_GetFileList`), `read`
    /// reads one by its path, `None` where it cannot.
    pub fn from_listing(
        list: impl Fn(&str, &str) -> Vec<String>,
        read: impl Fn(&str) -> Option<Vec<u8>>,
    ) -> Self {
        use crate::saber_definition::listed_files;
        let read = |directory: &str, extension: &str| -> Vec<(String, Vec<u8>)> {
            let names = list(directory, extension);
            let mut texts = Vec::new();
            for name in listed_files(names.iter().map(String::as_str)) {
                match read(&format!("{directory}/{name}")) {
                    Some(bytes) => texts.push((name.to_owned(), bytes)),
                    None => eprintln!("BG_VehicleLoadParms: error reading file: {name}"),
                }
            }
            texts
        };
        let (vehicles, weapons) = (
            read("ext_data/vehicles", ".veh"),
            read("ext_data/vehicles/weapons", ".vwp"),
        );
        Self::load(
            vehicles
                .iter()
                .map(|(name, text)| (name.as_str(), text.as_slice())),
            weapons
                .iter()
                .map(|(name, text)| (name.as_str(), text.as_slice())),
        )
        .unwrap_or_else(|error| {
            eprintln!("BG_VehicleLoadParms: {error}");
            Self::default()
        })
    }

    /// `BG_VehicleLoadParms` (with `BG_VehWeaponLoadParms`) over the files' contents, each
    /// list in its listing's order (`FS_GetFileList`, [`crate::saber_definition::listed_files`]).
    pub fn load<'a>(
        vehicles: impl IntoIterator<Item = (&'a str, &'a [u8])>,
        weapons: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    ) -> Result<Self, VehicleFilesTooLarge> {
        let vehicles = join(vehicles, MAX_VEHICLE_DATA_SIZE)
            .map_err(|file| VehicleFilesTooLarge::Vehicles { file })?;
        let weapons = join(weapons, MAX_WEAPON_DATA_SIZE)
            .map_err(|file| VehicleFilesTooLarge::Weapons { file })?;
        Ok(Self { vehicles, weapons })
    }

    /// The joined vehicle text (`VehicleParms`).
    pub fn vehicle_text(&self) -> &[u8] {
        &self.vehicles
    }

    /// The name of every block in the vehicle text, in its order, as the game's parser
    /// walks it (a token, then a braced section skipped).
    pub fn vehicle_names(&self) -> Vec<Vec<u8>> {
        let mut parser = TextParser::new(&self.vehicles);
        let mut names = Vec::new();
        loop {
            let token = parser.parse_ext(true);
            if token.is_empty() {
                return names;
            }
            names.push(token.to_vec());
            parser.skip_braced_section(0);
        }
    }
}

/// What the vehicle table registers, as the game module does: `G_ModelIndex`,
/// `G_SoundIndex` and `G_EffectIndex` (each answering the configstring index, 0 for an
/// empty name), and the console (`Com_Printf`).
pub trait VehicleRegistry {
    fn model_index(&mut self, name: &[u8]) -> i32;
    fn sound_index(&mut self, name: &[u8]) -> i32;
    fn effect_index(&mut self, name: &[u8]) -> i32;
    fn print(&mut self, text: &str);
}

/// How many vehicles and vehicle weapons a table holds, its empty default entries
/// included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VehicleCapacity {
    pub vehicles: usize,
    pub weapons: usize,
}

impl VehicleCapacity {
    /// `MAX_VEHICLES`, `MAX_VEH_WEAPONS`.
    pub const REFERENCE: Self = Self {
        vehicles: 16,
        weapons: 16,
    };
}

/// `g_vehicleInfo`, `g_vehWeaponInfo` and their counts, for one level.
#[derive(Clone, Debug)]
pub struct VehicleTable {
    files: std::sync::Arc<VehicleFiles>,
    capacity: VehicleCapacity,
    /// Loaded vehicles, entry 0 the default (`numVehicles` is their count).
    vehicles: Vec<VehicleInfo>,
    /// Loaded weapons, entry 0 the default (`numVehicleWeapons` is their count), and the
    /// next slot as a failed load left it: the reference loads into it again without
    /// clearing it.
    weapons: Vec<VehicleWeaponInfo>,
    next_weapon: Option<VehicleWeaponInfo>,
}

impl VehicleTable {
    /// The table as `G_InitGame` leaves it (`BG_VehicleLoadParms`): the default vehicle
    /// and the default weapon only.
    pub fn new(files: std::sync::Arc<VehicleFiles>, capacity: VehicleCapacity) -> Self {
        Self {
            files,
            capacity,
            vehicles: vec![VehicleInfo::default()],
            weapons: vec![VehicleWeaponInfo::default()],
            next_weapon: None,
        }
    }

    /// Back to the default entries alone, as a new level's module starts.
    pub fn clear(&mut self) {
        self.vehicles.truncate(1);
        self.weapons.truncate(1);
        self.next_weapon = None;
    }

    /// The vehicle at `index`.
    pub fn vehicle(&self, index: usize) -> Option<&VehicleInfo> {
        self.vehicles.get(index)
    }

    /// The loaded vehicles, the default first.
    pub fn vehicles(&self) -> &[VehicleInfo] {
        &self.vehicles
    }

    /// The loaded weapons, the default first.
    pub fn weapons(&self) -> &[VehicleWeaponInfo] {
        &self.weapons
    }

    /// The kind (`type`) the vehicle `name` would have, without loading it: a loaded
    /// vehicle's, or the block's last `type` key read as the loader reads it — nothing
    /// registered, nothing printed. `None` for a block that is not there or never ends.
    pub fn definition_kind(&self, name: &[u8]) -> Option<i32> {
        if let Some(vehicle) = self.vehicles.iter().find(|vehicle| {
            vehicle
                .name
                .as_deref()
                .is_some_and(|known| known.eq_ignore_ascii_case(name))
        }) {
            return Some(vehicle.kind);
        }
        let mut parser = TextParser::new(&self.files.vehicles);
        if name.is_empty() || !find_block(&mut parser, name) {
            return None;
        }
        let mut found = kind::NONE;
        loop {
            parser.skip_rest_of_line();
            let token = parser.parse_ext(true);
            if token.is_empty() {
                return None;
            }
            if token.eq_ignore_ascii_case(b"}") {
                return Some(found);
            }
            let is_type = truncated(token, NAME_BUFFER).eq_ignore_ascii_case(b"type");
            let value = parser.parse_ext(true);
            if is_type && !value.is_empty() {
                found = id_for_string(&kind::NAMES, truncated(value, VALUE_BUFFER));
            }
        }
    }

    /// `BG_VehicleGetIndex` (`VEH_VehicleIndexForName`): the loaded vehicle whose `name`
    /// key is `name`, in any case; else the block named `name` loaded. `None` is
    /// `VEHICLE_NONE`.
    pub fn index_for_name(
        &mut self,
        name: &[u8],
        registry: &mut impl VehicleRegistry,
    ) -> Option<usize> {
        if name.is_empty() {
            registry.print("^1ERROR: Trying to read Vehicle with no name!\n");
            return None;
        }
        if let Some(found) = self.vehicles.iter().position(|vehicle| {
            vehicle
                .name
                .as_deref()
                .is_some_and(|known| known.eq_ignore_ascii_case(name))
        }) {
            return Some(found);
        }
        if self.vehicles.len() >= self.capacity.vehicles {
            registry.print(&format!(
                "^1ERROR: Too many Vehicles (max {}), aborting load on {}!\n",
                self.capacity.vehicles,
                text(name)
            ));
            return None;
        }
        let loaded = self.load_vehicle(name, registry);
        if loaded.is_none() {
            registry.print(&format!(
                "^1ERROR: Could not find Vehicle {}!\n",
                text(name)
            ));
        }
        loaded
    }

    /// `VEH_LoadVehicle`: the block named `name` read into the next entry.
    fn load_vehicle(&mut self, name: &[u8], registry: &mut impl VehicleRegistry) -> Option<usize> {
        let files = std::sync::Arc::clone(&self.files);
        let mut parser = TextParser::new(&files.vehicles);
        if !find_block(&mut parser, name) {
            return None;
        }
        let mut vehicle = VehicleInfo::default();
        // Weapons are read after the block: their own parse must not run inside this one.
        let mut deferred: Vec<(&'static str, Vec<u8>)> = Vec::new();
        loop {
            parser.skip_rest_of_line();
            let token = parser.parse_ext(true);
            if token.is_empty() {
                registry.print(&format!(
                    "^1ERROR: unexpected EOF while parsing Vehicle '{}'\n",
                    text(name)
                ));
                return None;
            }
            if token.eq_ignore_ascii_case(b"}") {
                break;
            }
            let key = truncated(token, NAME_BUFFER);
            let value = parser.parse_ext(true);
            if value.is_empty() {
                registry.print(&format!(
                    "^1ERROR: Vehicle token '{}' has no value!\n",
                    text(key)
                ));
            } else if let Some(deferred_key) = DEFERRED_WEAPON_KEYS
                .iter()
                .find(|known| known.as_bytes().eq_ignore_ascii_case(key))
            {
                let slot = deferred.iter().position(|(known, _)| known == deferred_key);
                let value = truncated(value, NAME_BUFFER).to_vec();
                match slot {
                    Some(at) => deferred[at].1 = value,
                    None => deferred.push((deferred_key, value)),
                }
            } else if !self.parse_vehicle_parm(&mut vehicle, key, value, registry) {
                registry.print(&format!(
                    "^1ERROR: Unknown Vehicle key/value pair '{}', '{}'!\n",
                    text(key),
                    text(value)
                ));
            }
        }
        // `weap1`, `weap2`, then the muzzles, whatever order the block named them in.
        for key in DEFERRED_WEAPON_KEYS {
            if let Some((_, value)) = deferred.iter().find(|(known, _)| *known == key) {
                self.parse_vehicle_parm(&mut vehicle, key.as_bytes(), value, registry);
            }
        }
        for health in [
            &mut vehicle.health_front,
            &mut vehicle.health_back,
            &mut vehicle.health_right,
            &mut vehicle.health_left,
        ] {
            if *health == 0 {
                *health = vehicle.armor / 4;
            }
        }
        if let Some(model) = &vehicle.model {
            vehicle.model_index = registry
                .model_index(&[b"models/players/", model.as_slice(), b"/model.glm"].concat());
        }
        clamp(&mut vehicle);
        if vehicle.explosion_damage != 0 {
            registry.effect_index(b"ships/ship_explosion_mark");
        }
        if vehicle.flammable {
            registry.sound_index(b"sound/vehicles/common/fire_lp.wav");
        }
        if vehicle.hover_height > 0.0 {
            registry.effect_index(b"ships/swoop_dust");
        }
        registry.effect_index(b"volumetric/black_smoke");
        registry.effect_index(b"ships/fire");
        registry.sound_index(b"sound/vehicles/common/release.wav");
        self.vehicles.push(vehicle);
        Some(self.vehicles.len() - 1)
    }

    /// `BG_ParseVehicleParm`: whether the key is known.
    fn parse_vehicle_parm(
        &mut self,
        vehicle: &mut VehicleInfo,
        key: &[u8],
        value: &[u8],
        registry: &mut impl VehicleRegistry,
    ) -> bool {
        let Some(field) = find(VEHICLE_FIELDS, key) else {
            return false;
        };
        let value = truncated(value, VALUE_BUFFER);
        if let Slot::Weapon(slot) = (field.slot)(vehicle) {
            *slot = self.weapon_index_for_name(value, registry);
            return true;
        }
        apply(field, vehicle, value, ValueMessages::VEHICLE, registry);
        true
    }

    /// `VEH_VehWeaponIndexForName`: the loaded weapon named `name`, else loaded; -1 is
    /// `VEH_WEAPON_NONE`.
    fn weapon_index_for_name(&mut self, name: &[u8], registry: &mut impl VehicleRegistry) -> i32 {
        if name.is_empty() {
            registry.print("^1ERROR: Trying to read Vehicle Weapon with no name!\n");
            return -1;
        }
        if let Some(found) = self.weapons.iter().position(|weapon| {
            weapon
                .name
                .as_deref()
                .is_some_and(|known| known.eq_ignore_ascii_case(name))
        }) {
            return found as i32;
        }
        if self.weapons.len() >= self.capacity.weapons {
            registry.print(&format!(
                "^1ERROR: Too many Vehicle Weapons (max 16), aborting load on {}!\n",
                text(name)
            ));
            return -1;
        }
        let loaded = self.load_weapon(name, registry);
        if loaded == -1 {
            registry.print(&format!(
                "^1ERROR: Could not find Vehicle Weapon {}!\n",
                text(name)
            ));
        }
        loaded
    }

    /// `VEH_LoadVehWeapon`. A name not found answers 0, the default weapon (the
    /// reference's `return qfalse`); a broken block answers -1 and leaves the slot as far
    /// as it was read, for the next load to read over.
    fn load_weapon(&mut self, name: &[u8], registry: &mut impl VehicleRegistry) -> i32 {
        let files = std::sync::Arc::clone(&self.files);
        let mut parser = TextParser::new(&files.weapons);
        loop {
            let token = parser.parse_ext(true);
            if token.is_empty() {
                return 0;
            }
            if token.eq_ignore_ascii_case(name) {
                break;
            }
            parser.skip_braced_section(0);
        }
        if !parser.is_live() {
            return 0;
        }
        let token = parser.parse_ext(true);
        if token.is_empty() || !token.eq_ignore_ascii_case(b"{") {
            return -1;
        }
        let mut weapon = self.next_weapon.take().unwrap_or_default();
        loop {
            parser.skip_rest_of_line();
            let token = parser.parse_ext(true);
            if token.is_empty() {
                registry.print(&format!(
                    "^1ERROR: unexpected EOF while parsing Vehicle Weapon '{}'\n",
                    text(name)
                ));
                self.next_weapon = Some(weapon);
                return -1;
            }
            if token.eq_ignore_ascii_case(b"}") {
                break;
            }
            let key = truncated(token, NAME_BUFFER);
            let value = parser.parse_ext(true);
            if value.is_empty() {
                registry.print(&format!(
                    "^1ERROR: Vehicle Weapon token '{}' has no value!\n",
                    text(key)
                ));
            } else if let Some(field) = find(WEAPON_FIELDS, key) {
                apply(
                    field,
                    &mut weapon,
                    truncated(value, VALUE_BUFFER),
                    ValueMessages::WEAPON,
                    registry,
                );
            } else {
                registry.print(&format!(
                    "^1ERROR: Unknown Vehicle Weapon key/value pair '{}','{}'!\n",
                    text(key),
                    text(value)
                ));
            }
        }
        self.weapons.push(weapon);
        (self.weapons.len() - 1) as i32
    }
}

/// The keys `VEH_LoadVehicle` sets aside for after the block, in the order it reads them.
const DEFERRED_WEAPON_KEYS: [&str; 2 + NAMED_MUZZLES] = [
    "weap1",
    "weap2",
    "weapMuzzle1",
    "weapMuzzle2",
    "weapMuzzle3",
    "weapMuzzle4",
    "weapMuzzle5",
    "weapMuzzle6",
    "weapMuzzle7",
    "weapMuzzle8",
    "weapMuzzle9",
    "weapMuzzle10",
];

/// The search of `VEH_LoadVehicle` and `VEH_LoadVehWeapon`: past the block named `name`'s
/// name and its `{`. A name not found, or not followed by `{`, is not loaded.
pub(crate) fn find_block(parser: &mut TextParser<'_>, name: &[u8]) -> bool {
    loop {
        let token = parser.parse_ext(true);
        if token.is_empty() {
            return false;
        }
        if token.eq_ignore_ascii_case(name) {
            break;
        }
        parser.skip_braced_section(0);
    }
    parser.is_live() && parser.parse_ext(true).eq_ignore_ascii_case(b"{")
}

/// The one message a table's vector reading prints.
struct ValueMessages {
    vector: &'static str,
}

impl ValueMessages {
    const VEHICLE: Self = Self {
        vector: "^3BG_ParseVehicleParm: VEC3 sscanf() failed to read 3 floats ('angle' key bug?)\n",
    };
    const WEAPON: Self = Self {
        vector: "^3BG_ParseVehWeaponParm: VEC3 sscanf() failed to read 3 floats ('angle' key bug?)\n",
    };
}

/// A value written to its field, as `BG_ParseVehicleParm` and `BG_ParseVehWeaponParm` do
/// in the game module (weapons are the caller's).
fn apply<T>(
    field: &Field<T>,
    record: &mut T,
    value: &[u8],
    messages: ValueMessages,
    registry: &mut impl VehicleRegistry,
) {
    match (field.slot)(record) {
        Slot::Int(slot) => *slot = crate::userinfo::atoi(value),
        Slot::Float(slot) => *slot = atof(value),
        Slot::Bool(slot) => *slot = atof(value) != 0.0,
        Slot::BoolInFloat(slot) => *slot = f32::from_bits(u32::from(atof(value) != 0.0)),
        Slot::Text(slot) => {
            if slot.is_none() {
                *slot = Some(value.to_vec());
            }
        }
        Slot::Vector(slot) => {
            let (read, vector) = scan_floats(value);
            if read == 3 {
                *slot = vector;
            } else {
                registry.print(messages.vector);
                *slot = [0.0; 3];
            }
        }
        Slot::VehicleKind(slot) => *slot = id_for_string(&kind::NAMES, value),
        Slot::Animation(slot) => *slot = id_for_string(crate::legacy_animation::NAMES, value),
        Slot::Model(slot) => *slot = registry.model_index(value),
        Slot::Effect(slot) => *slot = registry.effect_index(value),
        Slot::Sound(slot) => *slot = registry.sound_index(value),
        Slot::Weapon(_) | Slot::ClientOnly => {}
    }
}

/// `BG_VehicleClampData`.
fn clamp(vehicle: &mut VehicleInfo) {
    for axis in &mut vehicle.center_of_gravity {
        *axis = axis.clamp(-1.0, 1.0);
    }
    vehicle.max_passengers = vehicle
        .max_passengers
        .clamp(0, crate::vehicle_fields::MAX_PASSENGERS);
}

/// `GetIDForString`: the index of the name `value` in any case, -1 for none.
fn id_for_string(names: &[&str], value: &[u8]) -> i32 {
    names
        .iter()
        .position(|name| name.as_bytes().eq_ignore_ascii_case(value))
        .map_or(-1, |index| index as i32)
}

/// `sscanf(value, "%f %f %f")`: how many floats were read, and them. Each is `strtof`'s
/// longest decimal prefix after blanks (hexadecimal, `inf` and `nan` are not read).
fn scan_floats(value: &[u8]) -> (usize, [f32; 3]) {
    let mut out = [0.0; 3];
    let mut rest = value;
    for (read, slot) in out.iter_mut().enumerate() {
        rest = rest.trim_ascii_start();
        let length = decimal_prefix(rest);
        let Some(number) = std::str::from_utf8(&rest[..length])
            .ok()
            .and_then(|text| text.parse::<f32>().ok())
        else {
            return (read, out);
        };
        *slot = number;
        rest = &rest[length..];
    }
    (3, out)
}

/// The length of the longest decimal number at the start of `text` (sign, digits, point,
/// exponent), 0 without one.
fn decimal_prefix(text: &[u8]) -> usize {
    let digits = |from: usize| {
        text[from.min(text.len())..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count()
    };
    let mut end = usize::from(matches!(text.first(), Some(b'+' | b'-')));
    let whole = digits(end);
    end += whole;
    let mut fraction = 0;
    if text.get(end) == Some(&b'.') {
        fraction = digits(end + 1);
        end += 1 + fraction;
    }
    if whole + fraction == 0 {
        return 0;
    }
    if matches!(text.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(text.get(end + 1), Some(b'+' | b'-')));
        let exponent = digits(end + 1 + sign);
        if exponent > 0 {
            end += 1 + sign + exponent;
        }
    }
    end
}

/// `Q_strncpyz` into a buffer of `size`: at most `size - 1` bytes.
fn truncated(value: &[u8], size: usize) -> &[u8] {
    &value[..value.len().min(size - 1)]
}

fn text(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(bytes)
}
