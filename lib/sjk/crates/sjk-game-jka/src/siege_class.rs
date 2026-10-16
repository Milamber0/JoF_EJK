//! The classes a siege map's teams are made of (OpenJK `codemp/game/bg_saga.c`).
//!
//! Siege is the one stock game type where a player does not simply spawn with the usual
//! loadout: each side is a *theme* (a team file, `ext_data/Siege/Teams/*.team`) listing
//! up to [`MAX_SIEGE_CLASSES`] classes, and a class file (`ext_data/Siege/Classes/*.scl`)
//! says what that class carries, how much health and armour it has, which Force powers it
//! knows and at what rank, and — optionally — which model and saber it is forced to use.
//!
//! [`SiegeRegistry`] is `BG_SiegeLoadClasses` and `BG_SiegeLoadTeams`: every class file and
//! every team file the game ships, read in the listing's order with [`parse`] and
//! [`parse_team`] (`BG_SiegeParseClassFile`, `BG_SiegeParseTeamFile`), each through the
//! reference's own text routines ([`crate::siege_text`]). What the spawn hands a player
//! is [`loadout`]; which side a class name belongs to is [`SiegeTeams::team_for_class`].

use crate::siege_text::{self, FORCE_POWER_COUNT, TextError};

/// `MAX_SIEGE_CLASSES`: how many classes the game keeps, and how far a team's `classN`
/// keys are read.
pub const MAX_SIEGE_CLASSES: usize = 128;
/// `NUM_FORCE_POWERS` in multiplayer.
pub const NUM_FORCE_POWERS: usize = FORCE_POWER_COUNT;

/// `WP_MELEE`, `WP_SABER`, `WP_BRYAR_PISTOL`, `WP_ROCKET_LAUNCHER`, `WP_NUM_WEAPONS`.
const WP_MELEE: u32 = 2;
const WP_SABER: u32 = 3;
const WP_BRYAR_PISTOL: u32 = 4;
const WP_ROCKET_LAUNCHER: u32 = 11;
const WP_NUM_WEAPONS: u32 = 19;

/// `StanceTable` (`bg_saga.c:79-90`).
const STANCES: [(&str, i32); 8] = [
    ("SS_NONE", 0),
    ("SS_FAST", 1),
    ("SS_MEDIUM", 2),
    ("SS_STRONG", 3),
    ("SS_DESANN", 4),
    ("SS_TAVION", 5),
    ("SS_DUAL", 6),
    ("SS_STAFF", 7),
];

/// `bgSiegeClassFlagNames` (`bg_saga.c:66-77`): the `CFL_*` bits.
const CLASS_FLAGS: [(&str, i32); 8] = [
    ("CFL_MORESABERDMG", 0),
    ("CFL_STRONGAGAINSTPHYSICAL", 1),
    ("CFL_FASTFORCEREGEN", 2),
    ("CFL_STATVIEWER", 3),
    ("CFL_HEAVYMELEE", 4),
    ("CFL_SINGLE_ROCKET", 5),
    ("CFL_CUSTOMSKEL", 6),
    ("CFL_EXTRA_AMMO", 7),
];
/// `CFL_STATVIEWER`, `CFL_SINGLE_ROCKET` and `CFL_EXTRA_AMMO` as bit numbers.
pub const CFL_STATVIEWER: u32 = 3;
pub const CFL_SINGLE_ROCKET: u32 = 5;
pub const CFL_EXTRA_AMMO: u32 = 7;

/// `HoldableTable` (`bg_saga.c:138-152`).
const HOLDABLES: [(&str, i32); 12] = [
    ("HI_NONE", 0),
    ("HI_SEEKER", 1),
    ("HI_SHIELD", 2),
    ("HI_MEDPAC", 3),
    ("HI_MEDPAC_BIG", 4),
    ("HI_BINOCULARS", 5),
    ("HI_SENTRY_GUN", 6),
    ("HI_JETPACK", 7),
    ("HI_HEALTHDISP", 8),
    ("HI_AMMODISP", 9),
    ("HI_EWEB", 10),
    ("HI_CLOAK", 11),
];

/// `PowerupTable` (`bg_saga.c:154-173`).
const POWERUPS: [(&str, i32); 16] = [
    ("PW_NONE", 0),
    ("PW_QUAD", 1),
    ("PW_BATTLESUIT", 2),
    ("PW_PULL", 3),
    ("PW_REDFLAG", 4),
    ("PW_BLUEFLAG", 5),
    ("PW_NEUTRALFLAG", 6),
    ("PW_SHIELDHIT", 7),
    ("PW_SPEEDBURST", 8),
    ("PW_DISINT_4", 9),
    ("PW_SPEED", 10),
    ("PW_CLOAKED", 11),
    ("PW_FORCE_ENLIGHTENED_LIGHT", 12),
    ("PW_FORCE_ENLIGHTENED_DARK", 13),
    ("PW_FORCE_BOON", 14),
    ("PW_YSALAMIRI", 15),
];

/// `classTitles` (`bg_saga.c:772-780`): the icon names a class's `class_shader` ends in,
/// which is how the game tells its basic player class (`SPC_*`).
const CLASS_TITLES: [&str; 6] = [
    "infantry",
    "vanguard",
    "support",
    "jedi_general",
    "demolitionist",
    "heavy_weapons",
];

/// One class as `BG_SiegeParseClassFile` (`bg_saga.c:782-1062`) leaves it.
#[derive(Clone, Debug, PartialEq)]
pub struct SiegeClass {
    /// What the team file and the `siegeclass` command name it by.
    pub name: String,
    /// `forcedModel` and `forcedSkin`: `None` when the file does not force one.
    pub model: Option<String>,
    pub skin: Option<String>,
    /// `saber1`, `saber2`: the hilts it forces.
    pub saber1: Option<String>,
    pub saber2: Option<String>,
    /// `saberStance`: one bit per style (`SS_*`) it allows; 0 allows any.
    pub saber_stance: i32,
    /// One bit per weapon it carries.
    pub weapons: u32,
    /// What it knows of each Force power, by `forcePowers_t` order.
    pub force_levels: [u8; NUM_FORCE_POWERS],
    /// `classflags`: the `CFL_*` bits.
    pub class_flags: u32,
    /// `maxhealth` and `starthealth` (which defaults to it).
    pub max_health: i32,
    pub start_health: i32,
    /// `maxarmor` and `startarmor` (each defaults to the other).
    pub max_armor: i32,
    pub start_armor: i32,
    /// A multiplier on its movement, which is why some classes run.
    pub speed: f32,
    /// `sabercolor`/`saber2color`, present only when the class forces one.
    pub saber_color: Option<i32>,
    pub saber2_color: Option<i32>,
    /// `invenItems`: the holdables it carries, one bit each (`stats[STAT_HOLDABLE_ITEMS]`).
    pub holdables: u32,
    /// `powerups`: the powerups it spawns with for good, one bit each.
    pub powerups: u32,
    /// `playerClass`: its basic class (`SPC_INFANTRY` .. `SPC_HEAVY_WEAPONS`), read from
    /// the end of its `class_shader`.
    pub player_class: i32,
}

/// Why a class or team file was refused: the reference's `Com_Error`s.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClassError {
    /// The text itself is malformed.
    Text(TextError),
    /// "Siege class without name entry", "… without weapons entry", "… without uishader
    /// entry", "Siege team with no name definition", "Team defined with no allowable
    /// classes".
    Missing(&'static str),
}

impl From<TextError> for ClassError {
    fn from(error: TextError) -> Self {
        Self::Text(error)
    }
}

/// An optional text value: empty when absent, as the reference keeps it.
fn optional(block: &[u8], key: &str) -> Result<Option<String>, TextError> {
    Ok(siege_text::paired(block, key)?.filter(|value| !value.is_empty()))
}

/// `BG_SiegeParseClassFile`: one `.scl` file into a class. The file is the whole text;
/// its `ClassInfo` group is what is read (a file without one is read whole, as the
/// reference's buffer is then left as it was).
pub fn parse(text: &str) -> Result<SiegeClass, ClassError> {
    let bytes = text.as_bytes();
    let block = siege_text::value_group(bytes, b"ClassInfo")?.unwrap_or_else(|| bytes.to_vec());
    let block = block.as_slice();
    let name = siege_text::paired(block, "name")?
        .ok_or(ClassError::Missing("Siege class without name entry"))?;
    let model = optional(block, "model")?;
    let skin = optional(block, "skin")?;
    let saber1 = optional(block, "saber1")?;
    let saber2 = optional(block, "saber2")?;
    let saber_stance = siege_text::paired_value(block, b"saberstyle")?
        .map_or(0, |value| siege_text::generic_table(&value, &STANCES, true));
    let saber_color =
        siege_text::paired_value(block, b"sabercolor")?.map(|value| crate::userinfo::atoi(&value));
    let saber2_color =
        siege_text::paired_value(block, b"saber2color")?.map(|value| crate::userinfo::atoi(&value));
    let weapons = siege_text::paired_value(block, b"weapons")?
        .ok_or(ClassError::Missing("Siege class without weapons entry"))?;
    let mut weapons =
        siege_text::generic_table(&weapons, &crate::weapon_data::WEAPON_NAMES, true) as u32;
    // "make sure it has melee if there's no saber"
    if weapons & (1 << WP_SABER) == 0 {
        weapons |= 1 << WP_MELEE;
    }
    let force_levels = siege_text::paired_value(block, b"forcepowers")?
        .map_or([0; NUM_FORCE_POWERS], |value| {
            siege_text::force_powers(&value)
        });
    let class_flags = siege_text::paired_value(block, b"classflags")?.map_or(0, |value| {
        siege_text::generic_table(&value, &CLASS_FLAGS, true)
    }) as u32;
    let number = |key: &str| -> Result<Option<i32>, TextError> {
        Ok(siege_text::paired_value(block, key.as_bytes())?
            .map(|value| crate::userinfo::atoi(&value)))
    };
    let max_health = number("maxhealth")?.unwrap_or(100);
    let start_health = number("starthealth")?.unwrap_or(max_health);
    let mut max_armor = number("maxarmor")?.unwrap_or(0);
    let start_armor = match number("startarmor")? {
        Some(start) => {
            // "if they didn't specify a damn max armor then use this."
            if max_armor == 0 {
                max_armor = start;
            }
            start
        }
        None => max_armor,
    };
    let speed = siege_text::paired_value(block, b"speed")?
        .map_or(1.0, |value| crate::text_parse::atof(&value));
    if siege_text::paired_value(block, b"uishader")?.is_none() {
        return Err(ClassError::Missing("Siege class without uishader entry"));
    }
    // The game's own build reads the base class off the end of the icon's name
    // (`bg_saga.c:980-1010`); a name that ends in none of them is infantry. A class
    // without a `class_shader` keeps what the slot held, which a fresh game has as 0.
    let mut player_class = 0;
    if let Some(shader) = siege_text::paired_value(block, b"class_shader")? {
        let mut index = 0;
        while index < CLASS_TITLES.len() {
            let title = CLASS_TITLES[index].as_bytes();
            if title.len() > shader.len() {
                break;
            }
            if &shader[shader.len() - title.len()..] == title {
                player_class = index as i32;
                break;
            }
            index += 1;
        }
        if index >= CLASS_TITLES.len() {
            player_class = 0;
        }
    }
    let holdables = siege_text::paired_value(block, b"holdables")?.map_or(0, |value| {
        siege_text::generic_table(&value, &HOLDABLES, true)
    }) as u32;
    let powerups = siege_text::paired_value(block, b"powerups")?.map_or(0, |value| {
        siege_text::generic_table(&value, &POWERUPS, true)
    }) as u32;
    Ok(SiegeClass {
        name,
        model,
        skin,
        saber1,
        saber2,
        saber_stance,
        weapons,
        force_levels: force_levels.map(|level| level as u8),
        class_flags,
        max_health,
        start_health,
        max_armor,
        start_armor,
        speed,
        saber_color,
        saber2_color,
        holdables,
        powerups,
        player_class,
    })
}

/// One team theme as `BG_SiegeParseTeamFile` (`bg_saga.c:1263-1328`) leaves it: its name
/// and the class names its `Classes` group lists (`class1`, `class2`, … up to the first
/// one missing), which may name a class the game does not have.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiegeTheme {
    /// The name a map's `UseTeam` finds it by (`BG_SiegeFindTeamForTheme`).
    pub name: String,
    /// The class names, in the order a client's menu shows them.
    pub classes: Vec<String>,
}

/// `BG_SiegeParseTeamFile`.
pub fn parse_team(text: &str) -> Result<SiegeTheme, ClassError> {
    let bytes = text.as_bytes();
    let name = siege_text::paired(bytes, "name")?
        .ok_or(ClassError::Missing("Siege team with no name definition"))?;
    let mut classes = Vec::new();
    if let Some(block) = siege_text::value_group(bytes, b"Classes")? {
        for index in 1..MAX_SIEGE_CLASSES {
            let Some(class) = siege_text::paired(&block, &format!("class{index}"))? else {
                break;
            };
            classes.push(class);
        }
    }
    if classes.is_empty() {
        return Err(ClassError::Missing(
            "Team defined with no allowable classes",
        ));
    }
    Ok(SiegeTheme { name, classes })
}

/// Every class and team file the game ships, as `BG_SiegeLoadClasses` and
/// `BG_SiegeLoadTeams` keep them (`bgSiegeClasses`, `bgSiegeTeams`): in the order the
/// listing gives the files, a file the reference refuses left out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiegeRegistry {
    pub classes: Vec<SiegeClass>,
    pub themes: Vec<SiegeTheme>,
}

impl SiegeRegistry {
    /// Reads the class files and then the team files, each list in listing order; a file
    /// longer than the reference's buffer (4096 bytes for a class, 2048 for a team) is not
    /// read, as `FS_Open`'s caller skips it.
    pub fn load<'a>(
        class_files: impl IntoIterator<Item = &'a str>,
        team_files: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let mut registry = Self::default();
        for text in class_files {
            if text.len() >= 4_096 || registry.classes.len() >= MAX_SIEGE_CLASSES {
                continue;
            }
            match parse(text) {
                Ok(class) => registry.classes.push(class),
                Err(error) => eprintln!("siege: a class file is refused: {error:?}"),
            }
        }
        for text in team_files {
            if text.len() >= 2_048 {
                continue;
            }
            match parse_team(text) {
                Ok(theme) => registry.themes.push(theme),
                Err(error) => eprintln!("siege: a team file is refused: {error:?}"),
            }
        }
        registry
    }

    /// `BG_SiegeFindClassIndexByName`: the first class of that name, any case.
    pub fn class_index(&self, name: &str) -> Option<usize> {
        self.classes
            .iter()
            .position(|class| class.name.eq_ignore_ascii_case(name))
    }

    /// `BG_SiegeFindTeamForTheme`: the theme of that name, any case.
    pub fn theme(&self, name: &str) -> Option<&SiegeTheme> {
        self.themes
            .iter()
            .find(|theme| !theme.name.is_empty() && theme.name.eq_ignore_ascii_case(name))
    }

    /// The two sides a map's `UseTeam`s pick (`BG_SiegeSetTeamTheme`), each a list of
    /// class indices — `None` for a listed name no class has, as the reference keeps a
    /// null there.
    pub fn sides(&self, team1: Option<&str>, team2: Option<&str>) -> SiegeTeams {
        let side = |theme: Option<&str>| {
            theme
                .and_then(|name| self.theme(name))
                .map(|theme| SiegeSide {
                    classes: theme
                        .classes
                        .iter()
                        .map(|name| self.class_index(name))
                        .collect(),
                    names: theme.classes.clone(),
                })
        };
        SiegeTeams {
            team1: side(team1),
            team2: side(team2),
        }
    }
}

/// `SIEGETEAM_TEAM1` and `SIEGETEAM_TEAM2`, which are the red and blue of a siege map.
pub const SIEGETEAM_TEAM1: u8 = 1;
pub const SIEGETEAM_TEAM2: u8 = 2;

/// One side's theme as the game keeps it (`team1Theme`, `team2Theme`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiegeSide {
    /// The registry's class for each name the theme lists, `None` where there is none.
    pub classes: Vec<Option<usize>>,
    /// The names themselves.
    pub names: Vec<String>,
}

/// The two themes a siege map plays, `None` for a side whose theme was not found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiegeTeams {
    pub team1: Option<SiegeSide>,
    pub team2: Option<SiegeSide>,
}

impl SiegeTeams {
    /// `BG_SiegeFindThemeForTeam`.
    pub fn side(&self, team: i32) -> Option<&SiegeSide> {
        match team {
            1 => self.team1.as_ref(),
            2 => self.team2.as_ref(),
            _ => None,
        }
    }

    /// `G_TeamForSiegeClass` (`g_cmds.c:1125-1163`): which side a class name belongs to,
    /// searching team one's classes and then team two's. `None` when neither has it — or
    /// when team one has no theme at all, which the reference answers the same way.
    pub fn team_for_class(&self, registry: &SiegeRegistry, name: &str) -> Option<u8> {
        self.team1.as_ref()?;
        for (team, side) in [
            (SIEGETEAM_TEAM1, &self.team1),
            (SIEGETEAM_TEAM2, &self.team2),
        ] {
            let Some(side) = side else { continue };
            for class in side.classes.iter().take(MAX_SIEGE_CLASSES).flatten() {
                let class = &registry.classes[*class];
                if !class.name.is_empty() && class.name.eq_ignore_ascii_case(name) {
                    return Some(team);
                }
            }
        }
        None
    }

    /// `BG_SiegeCheckClassLegality`: whether `name` is one of `team`'s classes; when it is
    /// not, the side's first class is what the player is given instead. A spectator, or a
    /// side without a theme, is legal whatever it names.
    pub fn legal_class(
        &self,
        registry: &SiegeRegistry,
        team: i32,
        name: &str,
    ) -> Result<(), String> {
        let Some(side) = self.side(team) else {
            return Ok(());
        };
        for class in &side.classes {
            // The reference reads a null class's name here and would crash; a class the
            // theme names but the game lacks is simply not a match.
            if class.is_some_and(|class| registry.classes[class].name.eq_ignore_ascii_case(name)) {
                return Ok(());
            }
        }
        Err(side
            .classes
            .first()
            .copied()
            .flatten()
            .map_or_else(String::new, |class| registry.classes[class].name.clone()))
    }

    /// `ClientUserinfoChanged`'s class (`g_client.c:2217-2234`): the class named `name`
    /// (`sess.siegeClass`) for a player on `team` — a name no class has becomes the side's
    /// first class (`BG_SiegeCheckClassLegality`), a class the side does not have becomes
    /// its own kind on that side (`G_ValidateSiegeClassForTeam`). Returns the class's index
    /// (`client->siegeClass`) and the name the session keeps.
    pub fn resolve(
        &self,
        registry: &SiegeRegistry,
        name: &str,
        team: i32,
    ) -> (Option<usize>, String) {
        match registry.class_index(name) {
            None => {
                let name = self
                    .legal_class(registry, team, name)
                    .err()
                    .unwrap_or_else(|| name.to_owned());
                (registry.class_index(&name), name)
            }
            Some(index) => match self.validated_class(registry, team, Some(index)) {
                Some(new) => (Some(new), registry.classes[new].name.clone()),
                None => (Some(index), name.to_owned()),
            },
        }
    }

    /// `G_ValidateSiegeClassForTeam` (`g_saga.c:714-751`): the class a player moving to
    /// `team` keeps — its own when the side has it, else the side's last class of the same
    /// basic kind, else its first class. `None` leaves the player's class as it was.
    pub fn validated_class(
        &self,
        registry: &SiegeRegistry,
        team: i32,
        current: Option<usize>,
    ) -> Option<usize> {
        let current = current?;
        let side = self.side(team)?;
        let wanted = &registry.classes[current];
        let mut replacement: Option<usize> = None;
        for (index, class) in side.classes.iter().enumerate() {
            let Some(class) = class else { continue };
            let class = &registry.classes[*class];
            if class.name.eq_ignore_ascii_case(&wanted.name) {
                return None;
            }
            if class.player_class == wanted.player_class || replacement.is_none() {
                replacement = Some(index);
            }
        }
        let name = &side.names[replacement?];
        registry.class_index(name)
    }
}

/// What the spawn hands a player of a class (`ClientSpawn`, `g_client.c:3528-3620` and
/// `:3674-3722`): its weapons, the one it holds, the ammunition, its holdables and
/// powerups, and its health and armour.
#[derive(Clone, Debug, PartialEq)]
pub struct Loadout {
    /// `stats[STAT_WEAPONS]`.
    pub weapons: u32,
    /// `ps.weapon`: what it is holding when it appears.
    pub weapon: u32,
    /// `ps.ammo`, by ammunition type; `None` leaves a type as it was.
    pub ammo: Vec<(usize, i32)>,
    /// `EF_DOUBLE_AMMO`: a class with `CFL_EXTRA_AMMO` carries twice the maximum.
    pub double_ammo: bool,
    /// `stats[STAT_HOLDABLE_ITEMS]`: the holdables it carries.
    pub holdables: u32,
    /// The powerups it holds for good (`Q3_INFINITE`).
    pub powerups: u32,
    /// `stats[STAT_HEALTH]`, when the class names a start health (it always does: the
    /// parse defaults it to the maximum).
    pub health: Option<i32>,
    /// `stats[STAT_ARMOR]`.
    pub armor: i32,
    /// The Force rank it knows in each power.
    pub force_levels: [u8; NUM_FORCE_POWERS],
}

/// `ClientSpawn`'s siege branch. A saber class holds its saber; anything else holds the
/// highest-numbered weapon it owns — the reference's own way of picking the biggest gun —
/// falling back to the pistol and then to fists. Every weapon from the pistol up is filled
/// to its ammunition's maximum (twice it with `CFL_EXTRA_AMMO`), rockets to 10 (1 with
/// `CFL_SINGLE_ROCKET`).
pub fn loadout(class: &SiegeClass) -> Loadout {
    let weapons = class.weapons;
    let mut weapon = if weapons & (1 << WP_SABER) != 0 {
        WP_SABER
    } else if weapons & (1 << WP_BRYAR_PISTOL) != 0 {
        WP_BRYAR_PISTOL
    } else {
        WP_MELEE
    };
    let double_ammo = class.class_flags & (1 << CFL_EXTRA_AMMO) != 0;
    let mut ammo = Vec::new();
    for index in 0..WP_NUM_WEAPONS {
        if weapons & (1 << index) == 0 {
            continue;
        }
        if weapon != WP_SABER && index > weapon {
            weapon = index;
        }
        if index < WP_BRYAR_PISTOL {
            continue;
        }
        let Some(data) = crate::weapon_data::legacy_weapon_data(index as u8) else {
            continue;
        };
        let kind = data.ammo_index;
        let amount = if index == WP_ROCKET_LAUNCHER {
            if class.class_flags & (1 << CFL_SINGLE_ROCKET) != 0 {
                1
            } else {
                10
            }
        } else if double_ammo {
            crate::npc_combat::AMMO_MAXIMA
                .get(kind)
                .copied()
                .unwrap_or(0)
                * 2
        } else {
            crate::npc_combat::AMMO_MAXIMA
                .get(kind)
                .copied()
                .unwrap_or(0)
        };
        ammo.push((kind, amount));
    }
    Loadout {
        weapons,
        weapon,
        ammo,
        double_ammo,
        holdables: class.holdables,
        powerups: class.powerups,
        health: (class.start_health != 0).then_some(class.start_health),
        armor: class.start_armor,
        force_levels: class.force_levels,
    }
}

/// The model a class forces (`ClientUserinfoChanged`, `g_client.c:2243-2250`): the class's
/// `model`, exactly — the skin is the client's to apply from the class file.
pub fn forced_model(class: &SiegeClass) -> Option<&str> {
    class.model.as_deref()
}

/// `CS_SIEGE_STATE` (`bg_public.h:127`, `CS_AMBIENT_SET + MAX_AMBIENT_SETS`), which a
/// client reads the round's state from, and the four siege configstrings after it.
pub const CS_SIEGE_STATE: usize = 293;
pub const CS_SIEGE_OBJECTIVES: usize = 294;
pub const CS_SIEGE_TIMEOVERRIDE: usize = 295;
pub const CS_SIEGE_WINTEAM: usize = 296;
pub const CS_SIEGE_ICONS: usize = 297;
