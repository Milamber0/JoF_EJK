//! User-owned multiplayer profile and its legacy protocol-26 adapter.
//!
//! The fields correspond to OpenJK's archived `CVAR_USERINFO` registrations
//! in `codemp/client/cl_main.cpp:2839-2860`. Serialization order remains the
//! responsibility of `sjk-network`, while this module owns JKA value policy.

use crate::{ForceAllocation, ForceProfileError};
use sjk_network::LegacyUserInfo;
use std::fmt;

/// One of BaseJKA's six serialized saber-colour indices, or the JA+/TaystJK
/// custom-RGB index 6 (`SABER_RGB`, TaystJK `codemp/game/q_shared.h:357`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SaberColor {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    /// Blade tinted by the `cp_sbRGB1`/`cp_sbRGB2` packed colour.
    Rgb,
}

impl SaberColor {
    /// Parse a `color1`/`color2` cvar value.
    pub fn from_index(index: u8) -> Result<Self, PlayerProfileError> {
        match index {
            0 => Ok(Self::Red),
            1 => Ok(Self::Orange),
            2 => Ok(Self::Yellow),
            3 => Ok(Self::Green),
            4 => Ok(Self::Blue),
            5 => Ok(Self::Purple),
            6 => Ok(Self::Rgb),
            _ => Err(PlayerProfileError::InvalidSaberColor(index)),
        }
    }

    /// Legacy `saber_colors_t` numeric representation.
    pub const fn index(self) -> u8 {
        self as u8
    }
}

/// Pack an RGB triplet the way TaystJK stores `cp_sbRGB1`/`cp_sbRGB2`
/// (`codemp/cgame/cg_consolecmds.c:917-920`: `r | (g | b << 8) << 8`).
pub const fn pack_saber_rgb(rgb: [u8; 3]) -> u32 {
    rgb[0] as u32 | (rgb[1] as u32) << 8 | (rgb[2] as u32) << 16
}

/// Unpack a `cp_sbRGB1`/`cp_sbRGB2` (or configstring `c3`/`c4`) value;
/// zero reads as 255 = pure red, as in TaystJK
/// `codemp/cgame/cg_players.c:2188-2193`.
pub const fn unpack_saber_rgb(packed: u32) -> [u8; 3] {
    let full = if packed == 0 { 255 } else { packed };
    [
        (full & 255) as u8,
        ((full >> 8) & 255) as u8,
        ((full >> 16) & 255) as u8,
    ]
}

/// Persistent, user-selectable identity and gameplay profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerProfile {
    /// Visible multiplayer name.
    pub name: String,
    /// Legacy `model/skin` appearance selection.
    pub model: String,
    /// Voice-set sex token.
    pub sex: String,
    /// Primary saber definition name.
    pub saber1: String,
    /// Secondary saber definition name or `none`.
    pub saber2: String,
    /// Primary blade colour.
    pub color1: SaberColor,
    /// Secondary blade colour.
    pub color2: SaberColor,
    /// Custom primary blade tint used when `color1` is [`SaberColor::Rgb`].
    pub rgb1: [u8; 3],
    /// Custom secondary blade tint used when `color2` is [`SaberColor::Rgb`].
    pub rgb2: [u8; 3],
    /// Per-player RGB tint.
    pub character_color: [u8; 3],
    /// Selected Force rank, side, and power levels.
    pub force: ForceAllocation,
    /// Starting-health percentage requested from the server.
    pub handicap: u8,
    /// JoF EJK's worn hat and cape, carried after the colour digits of
    /// `color1` and `color2` (see [`crate::split_color_value`]).
    pub cosmetics: [Option<String>; 2],
}

impl Default for PlayerProfile {
    fn default() -> Self {
        Self {
            name: "Padawan".to_owned(),
            model: "kyle/default".to_owned(),
            sex: "male".to_owned(),
            saber1: "single_1".to_owned(),
            saber2: "none".to_owned(),
            color1: SaberColor::Blue,
            color2: SaberColor::Blue,
            rgb1: [255; 3],
            rgb2: [255; 3],
            character_color: [255; 3],
            force: ForceAllocation::default(),
            handicap: 100,
            cosmetics: [None, None],
        }
    }
}

impl PlayerProfile {
    /// Construct a profile from the text forms stored by the cvar registry.
    /// `color1` and `color2` are read as the legacy game reads them, the
    /// colour with `atoi` and a JoF EJK cosmetic name after its digits.
    #[allow(clippy::too_many_arguments)]
    pub fn from_cvar_values(
        name: &str,
        model: &str,
        sex: &str,
        saber1: &str,
        saber2: &str,
        color1: &str,
        color2: &str,
        saber_rgb: [i64; 2],
        character_color: [i64; 3],
        forcepowers: &str,
        handicap: i64,
    ) -> Result<Self, PlayerProfileError> {
        let (color1, hat) = crate::split_color_value(color1);
        let (color2, cape) = crate::split_color_value(color2);
        Ok(Self {
            name: required(name, "name")?,
            model: required(model, "model")?,
            sex: required(sex, "sex")?,
            saber1: required(saber1, "saber1")?,
            saber2: required(saber2, "saber2")?,
            color1: SaberColor::from_index(to_u8(color1, "color1")?)?,
            color2: SaberColor::from_index(to_u8(color2, "color2")?)?,
            rgb1: unpack_saber_rgb(to_u32(saber_rgb[0], "cp_sbRGB1")?),
            rgb2: unpack_saber_rgb(to_u32(saber_rgb[1], "cp_sbRGB2")?),
            character_color: [
                to_u8(character_color[0], "char_color_red")?,
                to_u8(character_color[1], "char_color_green")?,
                to_u8(character_color[2], "char_color_blue")?,
            ],
            force: ForceAllocation::parse(forcepowers)?,
            handicap: to_u8(handicap, "handicap")?,
            cosmetics: [hat.map(str::to_owned), cape.map(str::to_owned)],
        })
    }

    /// Adapt this profile and connection settings to the transport payload.
    pub fn legacy_userinfo(
        &self,
        rate: u32,
        snaps: u16,
        predict_items: bool,
        password: Option<String>,
    ) -> LegacyUserInfo {
        let rgb = |color: SaberColor, rgb| (color == SaberColor::Rgb).then(|| pack_saber_rgb(rgb));
        LegacyUserInfo {
            name: self.name.clone(),
            model: self.model.clone(),
            rate,
            snaps,
            forcepowers: self.force.encode(),
            color1: self.color1.index(),
            color2: self.color2.index(),
            handicap: self.handicap,
            sex: self.sex.clone(),
            predict_items,
            saber1: self.saber1.clone(),
            saber2: self.saber2.clone(),
            char_color: self.character_color,
            saber_rgb: [rgb(self.color1, self.rgb1), rgb(self.color2, self.rgb2)],
            cosmetics: self.cosmetics.clone(),
            // Set from the player's cvar and applied per server profile.
            plugin_disable: None,
            extension_overrides: Vec::new(),
            password,
            // Stamped per server when the connection is made.
            guid: None,
        }
    }
}

fn required(value: &str, name: &'static str) -> Result<String, PlayerProfileError> {
    if value.trim().is_empty() {
        Err(PlayerProfileError::Empty(name))
    } else {
        Ok(value.to_owned())
    }
}

fn to_u8(value: i64, name: &'static str) -> Result<u8, PlayerProfileError> {
    u8::try_from(value).map_err(|_| PlayerProfileError::OutOfRange(name, value))
}

fn to_u32(value: i64, name: &'static str) -> Result<u32, PlayerProfileError> {
    u32::try_from(value).map_err(|_| PlayerProfileError::OutOfRange(name, value))
}

/// Invalid persisted player-profile value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlayerProfileError {
    /// A required text cvar was empty.
    Empty(&'static str),
    /// A numeric cvar could not be represented by its wire type.
    OutOfRange(&'static str, i64),
    /// A saber colour was outside the six stock colours plus RGB.
    InvalidSaberColor(u8),
    /// The Force allocation string was invalid.
    Force(ForceProfileError),
}

impl From<ForceProfileError> for PlayerProfileError {
    fn from(error: ForceProfileError) -> Self {
        Self::Force(error)
    }
}

impl fmt::Display for PlayerProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid player profile: {self:?}")
    }
}

impl std::error::Error for PlayerProfileError {}
