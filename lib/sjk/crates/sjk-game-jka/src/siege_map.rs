//! What a siege map's own `.siege` file says (`InitSiegeMode`, OpenJK
//! `codemp/game/g_saga.c:118-398`): the two sides' group names, and for each side what
//! it must complete and how long it has, which team theme it plays, and the objectives
//! the round is made of; plus the round's start (`preround_state`,
//! `roundbegin_target`). Everything is read through [`crate::siege_text`], exactly where
//! and when the reference reads it — an objective's `final` and `target` at the moment
//! it is used, from its side's group ([`SiegeSide::objective`]).

use crate::siege_text::{self, TextError};

/// `MAX_SIEGE_INFO_SIZE`: a longer `.siege` file is not read, and the map is not a
/// siege map (`siege_valid` stays 0).
pub const MAX_SIEGE_INFO_SIZE: usize = 16_384;

/// One side of a siege map (`team1`/`team2` in its `Teams` group).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiegeSide {
    /// The side's group name, as `Teams` (or the `g_siegeTeam1/2` override) names it.
    pub name: String,
    /// The side's group, tabs made spaces, when the file has it.
    pub group: Option<Vec<u8>>,
}

/// One objective's description in its side's group (`ObjectiveN`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectiveInfo {
    /// `final`: 1 wins the round, -1 does not count, 0 an ordinary one.
    pub final_flag: i32,
    /// `target`: the name used when it is completed, cut at a line end.
    pub target: Option<String>,
}

impl SiegeSide {
    /// A key of the side's own group (`BG_SiegeGetPairedValue(gParseObjectives, …)`).
    pub fn value(&self, key: &str) -> Result<Option<String>, TextError> {
        match &self.group {
            Some(group) => siege_text::paired(group, key),
            None => Ok(None),
        }
    }

    /// `ObjectiveN`'s description, when the side's group has that group
    /// (`siegeTriggerUse`, `g_saga.c:1107-1140`).
    pub fn objective(&self, number: i32) -> Result<Option<ObjectiveInfo>, TextError> {
        let Some(group) = &self.group else {
            return Ok(None);
        };
        let Some(objective) =
            siege_text::value_group(group, format!("Objective{number}").as_bytes())?
        else {
            return Ok(None);
        };
        let final_flag = siege_text::paired_value(&objective, b"final")?
            .map_or(0, |value| crate::userinfo::atoi(&value));
        let target = siege_text::paired(&objective, "target")?
            .map(|target| target.split(['\r', '\n']).next().unwrap_or("").to_owned());
        Ok(Some(ObjectiveInfo { final_flag, target }))
    }

    /// How many objectives the side lists: `Objective1`, `Objective2`, … up to the first
    /// missing (`g_saga.c:337-344`).
    pub fn objective_count(&self) -> Result<usize, TextError> {
        let Some(group) = &self.group else {
            return Ok(0);
        };
        let mut count = 0;
        while siege_text::value_group(group, format!("Objective{}", count + 1).as_bytes())?
            .is_some()
        {
            count += 1;
        }
        Ok(count)
    }
}

/// What a siege map brings to the game as it loads: its `.siege` file and every class and
/// team file the game ships (`BG_SiegeLoadClasses`, `BG_SiegeLoadTeams`, read by
/// `InitSiegeMode` each time the level starts; loaded once here, as the files do not
/// change while a server runs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiegeFiles {
    /// The map's `.siege` file.
    pub text: Vec<u8>,
    /// The classes and themes.
    pub registry: std::sync::Arc<crate::siege_class::SiegeRegistry>,
}

/// A map's `.siege` file as `InitSiegeMode` reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiegeMapInfo {
    /// The whole file (`siege_info`), which a few keys are read from at the time they
    /// are needed.
    pub text: Vec<u8>,
    /// `preround_state`: 0 keeps players spectating until the round begins.
    pub preround_state: i32,
    /// `team1` and `team2`.
    pub sides: [SiegeSide; 2],
    /// `RequiredObjectives`, by side.
    pub required: [i32; 2],
    /// `Timed`, in milliseconds, by side. Only one side may have a clock: a first side's
    /// is ignored when the second has one.
    pub time_limit: [i32; 2],
    /// `attackers`, by side.
    pub attackers: [i32; 2],
    /// `TeamIcon`, by side (the `team1_icon`/`team2_icon` cvars).
    pub icons: [Option<String>; 2],
    /// `UseTeam`: the team theme each side plays.
    pub themes: [Option<String>; 2],
}

/// Why a map's `.siege` file cannot be played.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapError {
    /// The text is malformed.
    Text(TextError),
    /// "Siege teams not defined".
    NoTeams,
    /// The file is longer than the reference reads.
    TooLong,
}

impl From<TextError> for MapError {
    fn from(error: TextError) -> Self {
        Self::Text(error)
    }
}

impl SiegeMapInfo {
    /// `InitSiegeMode`'s reading of the file. `overrides` are `g_siegeTeam1` and
    /// `g_siegeTeam2`: a value other than empty or `none` replaces the side's name.
    pub fn parse(text: &[u8], overrides: [&str; 2]) -> Result<Self, MapError> {
        if text.len() >= MAX_SIEGE_INFO_SIZE {
            return Err(MapError::TooLong);
        }
        let mut info = Self {
            text: text.to_vec(),
            ..Self::default()
        };
        if let Some(value) = siege_text::paired_value(text, b"preround_state")?
            && !value.is_empty()
        {
            info.preround_state = crate::userinfo::atoi(&value);
        }
        let teams = siege_text::value_group(text, b"Teams")?.ok_or(MapError::NoTeams)?;
        for (index, key) in ["team1", "team2"].into_iter().enumerate() {
            let chosen = overrides[index];
            info.sides[index].name = if !chosen.is_empty() && !chosen.eq_ignore_ascii_case("none") {
                chosen.to_owned()
            } else {
                siege_text::paired(&teams, key)?.unwrap_or_default()
            };
        }
        for index in 0..2 {
            info.sides[index].group =
                siege_text::value_group(text, info.sides[index].name.as_bytes())?;
        }
        // Team two first, as the reference reads it: its clock stands, and team one's is
        // refused when both have one.
        for index in [1, 0] {
            let side = &info.sides[index];
            info.icons[index] = side.value("TeamIcon")?;
            if let Some(value) = side.value("RequiredObjectives")? {
                info.required[index] = crate::userinfo::atoi(value.as_bytes());
            }
            if let Some(value) = side.value("Timed")?
                && !(index == 0 && info.time_limit[1] != 0)
            {
                info.time_limit[index] =
                    crate::userinfo::atoi(value.as_bytes()).wrapping_mul(1_000);
            }
            if let Some(value) = side.value("attackers")? {
                info.attackers[index] = crate::userinfo::atoi(value.as_bytes());
            }
        }
        for index in 0..2 {
            info.themes[index] = info.sides[index].value("UseTeam")?;
        }
        Ok(info)
    }

    /// A key at the file's own level (`roundbegin_target`, …).
    pub fn value(&self, key: &str) -> Option<String> {
        siege_text::paired(&self.text, key).ok().flatten()
    }

    /// The side of team `team` (1 or 2); any other team reads as team two, as the
    /// reference's `else` does.
    pub fn side(&self, team: i32) -> &SiegeSide {
        if team == 1 {
            &self.sides[0]
        } else {
            &self.sides[1]
        }
    }

    /// `CS_SIEGE_OBJECTIVES` as the round starts (`g_saga.c:360-376`): `t1`, a `-0` per
    /// objective of team one, `|t2` and team two's.
    pub fn objectives_config(&self) -> Vec<u8> {
        let mut text = b"t1".to_vec();
        for _ in 0..self.sides[0].objective_count().unwrap_or(0) {
            text.extend_from_slice(b"-0");
        }
        text.extend_from_slice(b"|t2");
        for _ in 0..self.sides[1].objective_count().unwrap_or(0) {
            text.extend_from_slice(b"-0");
        }
        text.truncate(1_023);
        text
    }
}
