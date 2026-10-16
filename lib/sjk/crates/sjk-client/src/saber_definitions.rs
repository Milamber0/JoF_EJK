//! Shared compatibility parser for retail and community `.sab` definitions.
//!
//! `WP_SetSaber` fills `saber_t` from `ext_data/sabers/*.sab`; both visible
//! hilt presentation and the codemp saber-hum loop must therefore consume the
//! same override-aware definition catalog.

use crate::catalog_tokens::tokenize;
use crate::player_profile::SaberColor;
use sjk_game_jka::pmove::saber_base::SaberHands;
use sjk_protocol::GameState;
use sjk_vfs::VirtualFileSystem;
use std::collections::BTreeMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

/// Compatibility fields consumed by current presentation and audio adapters.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacySaberDefinition {
    /// Definition token referenced by clientinfo `st`/`st2`.
    pub name: String,
    /// Menu-facing name from the definition's `name` keyword.
    pub display_name: String,
    /// Geometry family used by the multiplayer hilt chooser.
    pub saber_type: LegacySaberType,
    /// Hilt model selected by `saberModel`.
    pub model: String,
    /// Primary blade length in legacy units.
    pub length: f32,
    /// Primary blade render radius.
    pub radius: f32,
    /// Hum asset selected by `soundLoop`.
    pub sound_loop: String,
    /// Ignition sound selected by `soundOn` (lowercase), when authored.
    pub sound_on: Option<String>,
    /// Switching-off sound selected by `soundOff` (lowercase), when authored.
    pub sound_off: Option<String>,
    /// Optional model animation spin-sound override.
    pub sound_spin: Option<String>,
    /// Three authored animation swing variants (`swingSound1` through `3`).
    pub sound_swing: [Option<String>; 3],
    /// Number of active blade records (one through the codemp maximum of eight).
    pub num_blades: u8,
    /// Per-blade maximum lengths after the global and numbered overrides.
    pub blade_lengths: [f32; 8],
    /// Per-blade render radii after the global and numbered overrides.
    pub blade_radii: [f32; 8],
    /// First blade index that uses the secondary presentation style.
    pub blade_style2_start: u8,
    /// Primary blade trail style: normal, sword blur, or disabled.
    pub trail_style: u8,
    /// Secondary blade trail style selected by `blade_style2_start`.
    pub trail_style2: u8,
    /// Authored `noDlight` disables this hilt's presentation light.
    pub no_dlight: bool,
    /// Suppress world-contact marks and sparks (`noWallMarks`).
    pub no_wall_marks: bool,
    /// Definition-default blade colour.
    pub default_color: LegacySaberColor,
    /// Whether the raw `notInMP` keyword excludes this definition from UI lists.
    pub not_in_mp: bool,
    /// Whether `noRolls 1` sets `SFL_NO_ROLLS` for this hilt.
    pub no_rolls: bool,
    /// Whether `twoHanded 1` sets `SFL_TWO_HANDED`: such a hilt is never a second
    /// saber and drops one (`WP_SetSaber`, `bg_saberLoad.c:2237-2247`).
    pub two_handed: bool,
    /// Authored BG_AdjustClientSpeed multiplier, defaulting to one.
    pub move_speed_scale: f32,
    /// `animSpeedScale`: a saber attack's animation speed (`BG_SaberStartTransAnim`).
    pub anim_speed_scale: f32,
    /// Generic retail icon selected for this hilt family.
    pub icon: String,
}

/// Saber geometry families understood by BaseJKA definitions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacySaberType {
    /// A one-handed hilt (`SABER_SINGLE`).
    Single,
    /// A two-handed staff hilt (`SABER_STAFF`).
    Staff,
    /// A gameplay-defined extended family retained for community compatibility.
    Other(String),
}

/// A fixed or definition-randomized saber colour.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacySaberColor {
    /// One of the six `saber_colors_t` values.
    Fixed(SaberColor),
    /// `saberColor random`, chosen by gameplay when instantiated.
    Random,
}

type Definitions = BTreeMap<String, LegacySaberDefinition>;

/// Load the visible VFS union so mod PK3 definitions override retail files.
pub fn legacy_saber_definitions(vfs: &VirtualFileSystem) -> Result<Definitions, Box<dyn Error>> {
    let shared = shared_saber_definitions(vfs)?;
    Ok(Definitions::clone(&shared))
}

/// [`legacy_saber_definitions`], read once per set of mounts. Reading them lists every
/// file of every mount and parses every `.sab` (30 ms over a large `base`), and the
/// sounds and prediction ask on each clientinfo change, so a player joining used to
/// stall a frame; the game itself reads them once a map (`WP_SaberLoadParms`).
pub fn shared_saber_definitions(
    vfs: &VirtualFileSystem,
) -> Result<Arc<Definitions>, Box<dyn Error>> {
    /// File systems remembered: the world's and the few views made from it.
    const KEPT: usize = 4;
    static RECENT: Mutex<Vec<(Vec<u64>, Arc<Definitions>)>> = Mutex::new(Vec::new());
    let mounts = vfs.mount_identities();
    if let Some((_, definitions)) = RECENT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(known, _)| *known == mounts)
    {
        return Ok(Arc::clone(definitions));
    }
    // Read outside the lock; two readers of a new file system both read it.
    let definitions = Arc::new(read_saber_definitions(vfs)?);
    let mut recent = RECENT.lock().unwrap_or_else(|e| e.into_inner());
    if recent.len() >= KEPT {
        recent.remove(0);
    }
    recent.push((mounts, Arc::clone(&definitions)));
    Ok(definitions)
}

fn read_saber_definitions(vfs: &VirtualFileSystem) -> Result<Definitions, Box<dyn Error>> {
    let mut definitions = BTreeMap::new();
    for path in vfs.paths().into_iter().filter(|path| {
        path.as_str().starts_with("ext_data/sabers/") && path.as_str().ends_with(".sab")
    }) {
        let Some(asset) = vfs.read(path.as_str())? else {
            continue;
        };
        for definition in parse(&String::from_utf8_lossy(&asset.bytes)) {
            definitions.insert(definition.name.to_ascii_lowercase(), definition);
        }
    }
    Ok(definitions)
}

/// The saber definitions' ids (lowercase) in the order the game meets them:
/// the `ext_data/sabers/*.sab` files as `FS_GetFileList` lists them (the
/// highest-priority source first, each archive in its own order, a file name
/// once), joined in that order, then each file's definitions in order
/// (`WP_SaberLoadParms`, `bg_saberLoad.c:2357-2410`). EternalJK's and JoF
/// EJK's menus list the hilts in this order (`WP_SaberGetHiltInfo`, 2426-2507),
/// without sorting; an id met again keeps its first place.
pub fn legacy_saber_load_order(vfs: &VirtualFileSystem) -> Result<Vec<String>, Box<dyn Error>> {
    let mut order = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in vfs.list_files("ext_data/sabers", ".sab") {
        let Some(asset) = vfs.read(&format!("ext_data/sabers/{name}"))? else {
            continue;
        };
        for definition in parse(&String::from_utf8_lossy(&asset.bytes)) {
            let id = definition.name.to_ascii_lowercase();
            if seen.insert(id.clone()) {
                order.push(id);
            }
        }
    }
    Ok(order)
}

/// Resolve `SFL_NO_ROLLS` across both equipped sabers for one client.
///
/// `PM_TryRoll` rejects either flagged hilt (`bg_pmove.c:3594-3607`); the
/// keyword is parsed by `Saber_ParseNoRolls` (`bg_saberLoad.c:1245-1252`).
pub fn legacy_sabers_forbid_rolls(
    vfs: &VirtualFileSystem,
    game_state: &GameState,
    client_num: u16,
) -> bool {
    legacy_saber_movement(vfs, game_state, client_num).0
}

/// Resolve roll restrictions and both hands' moveSpeedScale and animSpeedScale values
/// off the frame path: BG_AdjustClientSpeed, bg_pmove.c:8505-8517;
/// BG_SaberStartTransAnim, bg_panimate.c:2693-2711.
pub fn legacy_saber_movement(
    vfs: &VirtualFileSystem,
    game_state: &GameState,
    client_num: u16,
) -> (bool, [f32; 2], [f32; 2]) {
    let names = crate::player_identity::legacy_client_saber_names(game_state, client_num);
    let Ok(definitions) = shared_saber_definitions(vfs) else {
        return (false, [1.0; 2], [1.0; 2]);
    };
    let mut no_rolls = false;
    let mut scales = [1.0; 2];
    let mut anim_scales = [1.0; 2];
    for (index, name) in names.into_iter().enumerate() {
        if let Some(definition) = name.and_then(|name| definitions.get(&name.to_ascii_lowercase()))
        {
            no_rolls |= definition.no_rolls;
            scales[index] = definition.move_speed_scale;
            anim_scales[index] = definition.anim_speed_scale;
        }
    }
    (no_rolls, scales, anim_scales)
}

/// What the client knows of the sabers one client holds, to work out `saberAnimLevelBase`
/// for its prediction (`cg_predict.c:1335-1347`): whether the first saber has blades and
/// whether a second one is held. `WP_SetSaber` drops a second saber beside a two-handed
/// one and loads `Kyle` for a `notInMP` or unknown name (`bg_saberLoad.c:2219-2247`).
/// A client without clientinfo gives the default, which knows nothing.
pub fn legacy_saber_hands(
    vfs: &VirtualFileSystem,
    game_state: &GameState,
    client_num: u16,
) -> SaberHands {
    if game_state
        .config_string(crate::player_identity::CS_PLAYERS + usize::from(client_num))
        .is_none_or(<[u8]>::is_empty)
    {
        return SaberHands::default();
    }
    let names = crate::player_identity::legacy_client_saber_names(game_state, client_num);
    let definitions = shared_saber_definitions(vfs).unwrap_or_default();
    saber_hands(&names, &definitions)
}

fn saber_hands(
    names: &[Option<String>; 2],
    definitions: &BTreeMap<String, LegacySaberDefinition>,
) -> SaberHands {
    let two_handed = |name: &str| {
        definitions
            .get(&name.to_ascii_lowercase())
            .filter(|definition| !definition.not_in_mp)
            .or_else(|| definitions.get("kyle"))
            .is_some_and(|definition| definition.two_handed)
    };
    // The first saber can never be removed; `none` leaves it unloaded.
    let first = names[0]
        .as_deref()
        .filter(|name| !name.eq_ignore_ascii_case("none") && !name.eq_ignore_ascii_case("remove"));
    let primary_blades = names[0].is_none() || first.is_some();
    let first_two_handed = first.is_some_and(two_handed);
    let second_saber =
        !first_two_handed && names[1].as_deref().is_some_and(|name| !two_handed(name));
    SaberHands {
        primary_blades,
        second_saber,
    }
}

fn parse(source: &str) -> Vec<LegacySaberDefinition> {
    let tokens = tokenize(source);
    let mut definitions = Vec::new();
    let mut index = 0;
    while index + 1 < tokens.len() {
        let name = tokens[index].clone();
        index += 1;
        if tokens.get(index).map(String::as_str) != Some("{") {
            index += 1;
            continue;
        }
        index += 1;
        // `WP_SaberSetDefaults`, bg_saberLoad.c:402-420.
        let mut display_name = "lightsaber".to_owned();
        let mut saber_type = LegacySaberType::Single;
        let mut model = "models/weapons2/saber/saber_w.glm".to_owned();
        let mut blade_lengths = [32.0_f32; 8];
        let mut blade_radii = [3.0_f32; 8];
        let mut sound_spin = None;
        let mut sound_swing = [None, None, None];
        let mut sound_loop = "sound/weapons/saber/saberhum3.wav".to_owned();
        let mut sound_on = None;
        let mut sound_off = None;
        let mut two_handed = false;
        let mut num_blades = 1_u8;
        let mut blade_style2_start = 0_u8;
        let mut trail_style = 0_u8;
        let mut trail_style2 = 0_u8;
        let mut no_dlight = false;
        let mut no_wall_marks = false;
        let mut default_color = LegacySaberColor::Fixed(SaberColor::Red);
        let mut not_in_mp = false;
        let mut no_rolls = false;
        let mut move_speed_scale = 1.0;
        let mut anim_speed_scale = 1.0;
        while let Some(key) = tokens.get(index) {
            if key == "}" {
                index += 1;
                break;
            }
            let Some(value) = tokens.get(index + 1) else {
                break;
            };
            match key.to_ascii_lowercase().as_str() {
                "name" => display_name.clone_from(value),
                "sabertype" => saber_type = parse_saber_type(value),
                "sabermodel" => model.clone_from(value),
                "numblades" => {
                    if let Ok(value) = value.parse::<u8>() {
                        if (1..=8).contains(&value) {
                            num_blades = value;
                        }
                    }
                }
                "sabercolor" => default_color = parse_color(value),
                "saberlength" => {
                    let length = value.parse::<f32>().unwrap_or(blade_lengths[0]).max(4.0);
                    blade_lengths.fill(length);
                }
                "saberradius" => {
                    let radius = value.parse::<f32>().unwrap_or(blade_radii[0]);
                    blade_radii.fill(radius.max(0.25));
                }
                "bladestyle2start" => {
                    blade_style2_start = value.parse::<u8>().unwrap_or(0).min(8);
                }
                "trailstyle" => trail_style = value.parse::<u8>().unwrap_or(0),
                "trailstyle2" => trail_style2 = value.parse::<u8>().unwrap_or(0),
                "nowallmarks" => no_wall_marks |= value.parse::<i32>().unwrap_or(0) != 0,
                "nodlight" => no_dlight |= value.parse::<i32>().unwrap_or(0) != 0,
                "soundloop" => sound_loop.clone_from(value),
                "soundon" => sound_on = Some(value.to_ascii_lowercase()),
                "soundoff" => sound_off = Some(value.to_ascii_lowercase()),
                // `Saber_ParseTwoHanded`, codemp/game/bg_saberLoad.c:884-892.
                "twohanded" => two_handed |= value.parse::<i32>().unwrap_or(0) != 0,
                "spinsound" => sound_spin = Some(value.to_ascii_lowercase()),
                "swingsound1" => sound_swing[0] = Some(value.to_ascii_lowercase()),
                "swingsound2" => sound_swing[1] = Some(value.to_ascii_lowercase()),
                "swingsound3" => sound_swing[2] = Some(value.to_ascii_lowercase()),
                "notinmp" => not_in_mp = value.parse::<i32>().unwrap_or(0) != 0,
                // `Saber_ParseNoRolls`, codemp/game/bg_saberLoad.c:1245-1252.
                "norolls" => no_rolls = value.parse::<i32>().unwrap_or(0) != 0,
                "movespeedscale" => move_speed_scale = value.parse::<f32>().unwrap_or(1.0),
                "animspeedscale" => anim_speed_scale = value.parse::<f32>().unwrap_or(1.0),
                numbered if numbered.starts_with("saberlength") => {
                    if let Ok(blade) = numbered[11..].parse::<usize>() {
                        if (2..=8).contains(&blade) {
                            blade_lengths[blade - 1] = value
                                .parse::<f32>()
                                .unwrap_or(blade_lengths[blade - 1])
                                .max(4.0);
                        }
                    }
                }
                numbered if numbered.starts_with("saberradius") => {
                    if let Ok(blade) = numbered[11..].parse::<usize>() {
                        if (2..=8).contains(&blade) {
                            blade_radii[blade - 1] = value
                                .parse::<f32>()
                                .unwrap_or(blade_radii[blade - 1])
                                .max(0.25);
                        }
                    }
                }
                _ => {}
            }
            index += 2;
        }
        definitions.push(LegacySaberDefinition {
            name,
            display_name,
            icon: icon_for_type(&saber_type).to_owned(),
            saber_type,
            model,
            length: blade_lengths[0],
            radius: blade_radii[0],
            sound_loop,
            sound_on,
            sound_off,
            sound_spin,
            sound_swing,
            num_blades,
            blade_lengths,
            blade_radii,
            blade_style2_start,
            trail_style,
            trail_style2,
            no_dlight,
            no_wall_marks,
            default_color,
            not_in_mp,
            no_rolls,
            two_handed,
            move_speed_scale,
            anim_speed_scale,
        });
    }
    definitions
}

fn parse_saber_type(value: &str) -> LegacySaberType {
    match value.to_ascii_lowercase().as_str() {
        "saber_single" => LegacySaberType::Single,
        "saber_staff" => LegacySaberType::Staff,
        _ => LegacySaberType::Other(value.to_owned()),
    }
}

fn parse_color(value: &str) -> LegacySaberColor {
    if value.eq_ignore_ascii_case("random") {
        return LegacySaberColor::Random;
    }
    let color = match value.to_ascii_lowercase().as_str() {
        "red" => SaberColor::Red,
        "orange" => SaberColor::Orange,
        "yellow" => SaberColor::Yellow,
        "green" => SaberColor::Green,
        "purple" => SaberColor::Purple,
        _ => SaberColor::Blue,
    };
    LegacySaberColor::Fixed(color)
}

fn icon_for_type(saber_type: &LegacySaberType) -> &'static str {
    match saber_type {
        LegacySaberType::Staff => "gfx/hud/w_icon_saberstaff.tga",
        LegacySaberType::Single | LegacySaberType::Other(_) => "gfx/hud/w_icon_lightsaber.tga",
    }
}

#[cfg(test)]
mod load_order_tests {
    use super::*;

    /// A `.sab` file defining single hilts named `ids`, in order.
    fn sab(ids: &[&str]) -> Vec<u8> {
        ids.iter()
            .map(|id| format!("{id}\n{{\n\tname \"{id}\"\n\tsaberType SABER_SINGLE\n}}\n"))
            .collect::<String>()
            .into_bytes()
    }

    #[test]
    fn hilts_come_in_the_games_file_order_then_each_files_own() {
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory(
            "base",
            [
                ("ext_data/sabers/a.sab", sab(&["kyle", "single_1"])),
                ("ext_data/sabers/z.sab", sab(&["single_2"])),
            ],
        )
        .unwrap();
        // Mounted later, so searched first: its files lead, and its a.sab
        // hides base's (single_1 is never met).
        vfs.mount_memory(
            "{JoF}pack",
            [
                ("ext_data/sabers/a.sab", sab(&["reborn", "Kyle"])),
                ("ext_data/sabers/jof.sab", sab(&["zroe", "akr"])),
            ],
        )
        .unwrap();
        assert_eq!(
            legacy_saber_load_order(&vfs).unwrap(),
            ["reborn", "kyle", "zroe", "akr", "single_2"]
        );
    }

    #[test]
    fn definitions_are_read_once_per_set_of_mounts() {
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory("base", [("ext_data/sabers/a.sab", sab(&["kyle"]))])
            .unwrap();
        let first = shared_saber_definitions(&vfs).unwrap();
        // A clone shares the mounts, so the catalog too.
        assert!(Arc::ptr_eq(
            &first,
            &shared_saber_definitions(&vfs.clone()).unwrap()
        ));
        // A new mount (a download, the next map) is read again.
        vfs.mount_memory("{JoF}pack", [("ext_data/sabers/jof.sab", sab(&["zroe"]))])
            .unwrap();
        let second = shared_saber_definitions(&vfs).unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
        assert!(second.contains_key("zroe") && second.contains_key("kyle"));
        assert!(!first.contains_key("zroe"));
    }
}

#[cfg(test)]
mod saber_hands_tests {
    use super::*;

    fn defs() -> BTreeMap<String, LegacySaberDefinition> {
        parse(
            "kyle
{
	saberType SABER_SINGLE
}
             staff_1
{
	saberType SABER_STAFF
	twoHanded 1
	numBlades 2
}
",
        )
        .into_iter()
        .map(|definition| (definition.name.to_ascii_lowercase(), definition))
        .collect()
    }

    fn names(first: Option<&str>, second: Option<&str>) -> [Option<String>; 2] {
        [first, second].map(|name| name.map(str::to_owned))
    }

    #[test]
    fn hands_of_a_pair_have_both_sabers() {
        let hands = saber_hands(&names(Some("kyle"), Some("kyle")), &defs());
        assert!(hands.primary_blades && hands.second_saber);
    }

    #[test]
    fn hands_of_a_single_saber_have_no_second() {
        let hands = saber_hands(&names(Some("kyle"), None), &defs());
        assert!(hands.primary_blades && !hands.second_saber);
    }

    #[test]
    fn hands_of_a_two_handed_staff_drop_the_second_saber() {
        let staff_first = saber_hands(&names(Some("staff_1"), Some("kyle")), &defs());
        assert!(staff_first.primary_blades && !staff_first.second_saber);
        let staff_second = saber_hands(&names(Some("kyle"), Some("staff_1")), &defs());
        assert!(!staff_second.second_saber);
    }

    #[test]
    fn hands_of_an_unnamed_or_unknown_saber_use_the_default_hilt() {
        let unnamed = saber_hands(&names(None, None), &defs());
        assert!(unnamed.primary_blades && !unnamed.second_saber);
        let unknown = saber_hands(&names(Some("nonesuch"), Some("nonesuch")), &defs());
        assert!(unknown.primary_blades && unknown.second_saber);
        let removed = saber_hands(&names(Some("none"), Some("kyle")), &defs());
        assert!(!removed.primary_blades && removed.second_saber);
    }
}
