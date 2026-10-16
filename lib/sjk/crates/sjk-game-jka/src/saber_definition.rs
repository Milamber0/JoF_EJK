//! Saber definitions (`codemp/game/bg_saberLoad.c`): what `ext_data/sabers/*.sab` says a
//! saber is, read the way the game reads it.
//!
//! - [`SaberParms`] is `saberParms`, every file compressed and joined in the listing's
//!   order (`WP_SaberLoadParms`).
//! - [`SaberParms::parse`] is `WP_SaberParseParms`: the defaults, the named saber found
//!   (or the default one), and its block read through the keyword table in
//!   [`crate::saber_keywords`].
//! - [`set_saber`] is `WP_SetSaber`: a hand's saber set by name, with the rules on
//!   removing one and on two-handed sabers.
//!
//! A saber's sounds are registered as it is parsed, as `BG_SoundIndex` does, and the
//! `random` colour draws from the game's generator: [`SaberParseHost`] gives both.

use crate::saber_info::{LS_INVALID, SaberInfo};
use crate::text_parse::{TextParser, compress, until_nul};

/// `DEFAULT_SABER`: the saber a player without a usable one carries.
pub const DEFAULT_SABER: &[u8] = b"Kyle";
/// `DEFAULT_SABER_MODEL`.
pub const DEFAULT_SABER_MODEL: &[u8] = b"models/weapons2/saber/saber_w.glm";
/// `MAX_BLADES`.
pub const MAX_BLADES: usize = 8;
/// `SABER_RADIUS_STANDARD`.
const SABER_RADIUS_STANDARD: f32 = 3.0;
/// `SABER_NAME_LENGTH`, `MAX_QPATH`: the name and path buffers, a byte of each for the NUL.
pub(crate) const SABER_NAME_LENGTH: usize = 64;
pub(crate) const MAX_QPATH: usize = 64;
/// `MAX_SABER_DATA_SIZE`: all the files together, compressed.
const MAX_SABER_DATA_SIZE: usize = 1024 * 1024;
/// `saberExtensionListBuf`'s size: the listing the files are read from.
const FILE_LIST_SIZE: usize = 2048;

/// `saberType_t`'s `SABER_SINGLE`, `SABER_STAFF`.
pub const SABER_SINGLE: i32 = 1;
pub const SABER_STAFF: i32 = 2;
/// `saber_colors_t`'s `SABER_RED` and `SABER_BLUE`.
pub const SABER_RED: i32 = 0;
pub const SABER_BLUE: i32 = 4;
/// `saber_styles_t`'s `SS_NONE`, `SS_TAVION`, `SS_DUAL`, and `SS_NUM_SABER_STYLES`.
pub const SS_NONE: i32 = 0;
pub const SS_FAST: i32 = 1;
pub const SS_TAVION: i32 = 5;
pub const SS_DUAL: i32 = 6;
pub const SS_NUM_SABER_STYLES: i32 = 8;
/// `SFL_TWO_HANDED`.
pub const SFL_TWO_HANDED: u32 = 1 << 4;

/// One blade's authored part (`bladeInfo_t`'s `color`, `radius`, `lengthMax`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BladeDefinition {
    /// `saber_colors_t`.
    pub color: i32,
    pub radius: f32,
    pub length_max: f32,
}

/// A saber as `WP_SaberParseParms` leaves `saberInfo_t`, less the blades' live state.
/// Pairs (`[_; 2]`) hold the primary blades' value, then the secondary blades'
/// (`bladeStyle2Start` on); sound fields are sound indices.
#[derive(Clone, Debug, PartialEq)]
pub struct SaberDefinition {
    /// The name it was asked for (`name`), and the one its block gives (`fullName`).
    pub name: Vec<u8>,
    pub full_name: Vec<u8>,
    /// `saberType_t`.
    pub saber_type: i32,
    /// The hilt; empty for a removed saber.
    pub model: Vec<u8>,
    /// `customSkin`'s file. The game registers it (`R_RegisterSkin`) and keeps the handle.
    pub skin: Vec<u8>,
    pub sound_on: u16,
    pub sound_loop: u16,
    pub sound_off: u16,
    pub num_blades: i32,
    pub blades: [BladeDefinition; MAX_BLADES],
    /// Bits of `1 << saber_styles_t`.
    pub styles_learned: i32,
    pub styles_forbidden: i32,
    pub max_chain: i32,
    /// Bits of `1 << forcePowers_t`.
    pub force_restrictions: i32,
    pub lock_bonus: i32,
    pub parry_bonus: i32,
    pub break_parry_bonus: [i32; 2],
    pub disarm_bonus: [i32; 2],
    pub single_blade_style: i32,
    /// `SFL_*`, `SFL2_*`.
    pub flags: u32,
    pub flags2: u32,
    pub spin_sound: u16,
    pub swing_sounds: [u16; 3],
    pub move_speed_scale: f32,
    pub anim_speed_scale: f32,
    /// `kataMove`, `lungeAtkMove`, `jumpAtkUpMove`, `jumpAtkFwdMove`, `jumpAtkBackMove`,
    /// `jumpAtkRightMove`, `jumpAtkLeftMove`: [`LS_INVALID`] to leave the style's.
    pub special_moves: [i32; 7],
    /// `readyAnim`, `drawAnim`, `putawayAnim`, `tauntAnim`, `bowAnim`, `meditateAnim`,
    /// `flourishAnim`, `gloatAnim`: -1 for the usual one.
    pub anims: [i32; 8],
    pub blade_style2_start: i32,
    pub trail_style: [i32; 2],
    pub hit_sounds: [[u16; 3]; 2],
    pub block_sounds: [[u16; 3]; 2],
    pub bounce_sounds: [[u16; 3]; 2],
    pub knockback_scale: [f32; 2],
    pub damage_scale: [f32; 2],
    pub splash_radius: [f32; 2],
    pub splash_damage: [i32; 2],
    pub splash_knockback: [f32; 2],
}

/// Where a parse registers sounds and draws random numbers.
pub trait SaberParseHost {
    /// `G_SoundIndex`.
    fn sound_index(&mut self, name: &[u8]) -> u16;
    /// `Q_irand`.
    fn irand(&mut self, low: i32, high: i32) -> i32;
}

/// Why a definition could not be read where the game would stop the map (`Com_Error`
/// with `ERR_DROP`). This game refuses the saber instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaberDefinitionError {
    /// `numBlades` outside one to [`MAX_BLADES`].
    IllegalBlades { saber: Vec<u8>, blades: i32 },
    /// The files together outgrow `MAX_SABER_DATA_SIZE`.
    TooLarge { file: String },
}

impl std::fmt::Display for SaberDefinitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IllegalBlades { saber, blades } => {
                write!(
                    f,
                    "saber {} has illegal number of blades ({blades}) max: {MAX_BLADES}",
                    String::from_utf8_lossy(saber)
                )
            }
            Self::TooLarge { file } => write!(
                f,
                "saber extensions (*.sab) are too large: ran out of space before reading {file}"
            ),
        }
    }
}

impl std::error::Error for SaberDefinitionError {}

/// `Q_strncpyz` into a buffer of `size`.
pub(crate) fn truncated(text: &[u8], size: usize) -> Vec<u8> {
    until_nul(text)[..until_nul(text).len().min(size - 1)].to_vec()
}

impl SaberDefinition {
    /// `WP_SaberSetDefaults`: the stock saber, its three sounds registered.
    pub fn defaults(host: &mut impl SaberParseHost) -> Self {
        let sound_on = host.sound_index(b"sound/weapons/saber/enemy_saber_on.wav");
        let sound_loop = host.sound_index(b"sound/weapons/saber/saberhum3.wav");
        let sound_off = host.sound_index(b"sound/weapons/saber/enemy_saber_off.wav");
        Self {
            name: DEFAULT_SABER.to_vec(),
            full_name: b"lightsaber".to_vec(),
            saber_type: SABER_SINGLE,
            model: DEFAULT_SABER_MODEL.to_vec(),
            skin: Vec::new(),
            sound_on,
            sound_loop,
            sound_off,
            num_blades: 1,
            blades: [BladeDefinition {
                color: SABER_RED,
                radius: SABER_RADIUS_STANDARD,
                length_max: 32.0,
            }; MAX_BLADES],
            styles_learned: 0,
            styles_forbidden: 0,
            max_chain: 0,
            force_restrictions: 0,
            lock_bonus: 0,
            parry_bonus: 0,
            break_parry_bonus: [0; 2],
            disarm_bonus: [0; 2],
            single_blade_style: SS_NONE,
            flags: 0,
            flags2: 0,
            spin_sound: 0,
            swing_sounds: [0; 3],
            move_speed_scale: 1.0,
            anim_speed_scale: 1.0,
            special_moves: [LS_INVALID; 7],
            anims: [-1; 8],
            blade_style2_start: 0,
            trail_style: [0; 2],
            hit_sounds: [[0; 3]; 2],
            block_sounds: [[0; 3]; 2],
            bounce_sounds: [[0; 3]; 2],
            knockback_scale: [0.0; 2],
            damage_scale: [1.0; 2],
            splash_radius: [0.0; 2],
            splash_damage: [0; 2],
            splash_knockback: [0.0; 2],
        }
    }

    /// A hand no saber was ever set in (the cleared client): no hilt, every field zero.
    pub fn empty() -> Self {
        Self {
            name: Vec::new(),
            full_name: Vec::new(),
            saber_type: 0,
            model: Vec::new(),
            skin: Vec::new(),
            sound_on: 0,
            sound_loop: 0,
            sound_off: 0,
            num_blades: 0,
            blades: [BladeDefinition {
                color: 0,
                radius: 0.0,
                length_max: 0.0,
            }; MAX_BLADES],
            styles_learned: 0,
            styles_forbidden: 0,
            max_chain: 0,
            force_restrictions: 0,
            lock_bonus: 0,
            parry_bonus: 0,
            break_parry_bonus: [0; 2],
            disarm_bonus: [0; 2],
            single_blade_style: 0,
            flags: 0,
            flags2: 0,
            spin_sound: 0,
            swing_sounds: [0; 3],
            move_speed_scale: 0.0,
            anim_speed_scale: 0.0,
            special_moves: [0; 7],
            anims: [0; 8],
            blade_style2_start: 0,
            trail_style: [0; 2],
            hit_sounds: [[0; 3]; 2],
            block_sounds: [[0; 3]; 2],
            bounce_sounds: [[0; 3]; 2],
            knockback_scale: [0.0; 2],
            damage_scale: [0.0; 2],
            splash_radius: [0.0; 2],
            splash_damage: [0; 2],
            splash_knockback: [0.0; 2],
        }
    }

    /// `WP_RemoveSaber`: the defaults again, named `none`, with no hilt.
    pub fn removed(host: &mut impl SaberParseHost) -> Self {
        Self {
            name: b"none".to_vec(),
            model: Vec::new(),
            ..Self::defaults(host)
        }
    }

    /// Whether the hand holds a saber at all (`model[0]`).
    pub fn is_held(&self) -> bool {
        !self.model.is_empty()
    }

    /// Its say in its wielder's moves.
    pub fn movement(&self) -> SaberInfo {
        let [
            kata_move,
            lunge_move,
            jump_up_move,
            jump_forward_move,
            jump_back_move,
            jump_right_move,
            jump_left_move,
        ] = self.special_moves;
        SaberInfo {
            kata_move,
            lunge_move,
            jump_up_move,
            jump_forward_move,
            jump_back_move,
            jump_right_move,
            jump_left_move,
            ready_anim: self.anims[0],
            draw_anim: self.anims[1],
            putaway_anim: self.anims[2],
            flags: self.flags,
            lock_bonus: self.lock_bonus,
        }
    }
}

/// `saberParms`: every saber file, compressed and joined.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaberParms {
    text: Vec<u8>,
}

/// `WP_SaberLoadParms`' listing: the names `FS_GetFileList` fits in its buffer, in order.
pub fn listed_files<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut used = 0;
    let mut kept = Vec::new();
    for name in names {
        let length = name.len() + 1;
        if used + length + 1 >= FILE_LIST_SIZE {
            break;
        }
        used += length;
        kept.push(name);
    }
    kept
}

impl SaberParms {
    /// `WP_SaberLoadParms` over the files' contents, in the listing's order.
    pub fn load<'a>(
        files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    ) -> Result<Self, SaberDefinitionError> {
        let mut text = Vec::new();
        for (name, contents) in files {
            let contents = until_nul(contents);
            if text.len() + contents.len() + 1 >= MAX_SABER_DATA_SIZE {
                return Err(SaberDefinitionError::TooLarge {
                    file: name.to_owned(),
                });
            }
            text.extend_from_slice(&compress(contents));
            // "get around the stupid problem of not having an endline at the bottom"
            text.push(b'\n');
        }
        Ok(Self { text })
    }

    /// The joined text.
    pub fn text(&self) -> &[u8] {
        &self.text
    }

    /// `WP_SaberParseParms`: the saber called `name` (any case; empty for the default),
    /// else the default one, else the bare defaults. Whether a block was read, and the
    /// saber.
    pub fn parse(
        &self,
        name: &[u8],
        host: &mut impl SaberParseHost,
    ) -> Result<(bool, SaberDefinition), SaberDefinitionError> {
        let mut saber = SaberDefinition::defaults(host);
        let name = until_nul(name);
        let mut tried_default = name.is_empty();
        let mut wanted = truncated(
            if tried_default { DEFAULT_SABER } else { name },
            SABER_NAME_LENGTH,
        );
        let mut parser = TextParser::new(&self.text);
        while parser.is_live() {
            let token = parser.parse_ext(true);
            if token.is_empty() {
                if tried_default {
                    return Ok((false, saber));
                }
                // Back to the start for the default one. The empty token is still
                // compared and a section skipped, as the reference's loop goes on.
                parser.restart();
                wanted = DEFAULT_SABER.to_vec();
                tried_default = true;
            }
            if token.eq_ignore_ascii_case(&wanted) {
                break;
            }
            parser.skip_braced_section(0);
        }
        if !parser.is_live() {
            return Ok((false, saber));
        }
        saber.name = wanted;
        if !parser.parse_literal(b"{") {
            return Ok((false, saber));
        }
        loop {
            let token = parser.parse_ext(true);
            if token.is_empty() {
                return Ok((false, saber));
            }
            if token.eq_ignore_ascii_case(b"}") {
                return Ok((true, saber));
            }
            match crate::saber_keywords::find(token) {
                Some(keyword) => keyword.apply(&mut saber, &mut parser, host)?,
                None => parser.skip_rest_of_line(),
            }
        }
    }

    /// `WP_SaberParseParm`: the first word after `key` in the block of the saber called
    /// `name`, which must be there by that name.
    pub fn parse_parm(&self, name: &[u8], key: &[u8]) -> Option<Vec<u8>> {
        let name = until_nul(name);
        if name.is_empty() {
            return None;
        }
        let mut parser = TextParser::new(&self.text);
        while parser.is_live() {
            let token = parser.parse_ext(true);
            if token.is_empty() {
                return None;
            }
            if token.eq_ignore_ascii_case(name) {
                break;
            }
            parser.skip_braced_section(0);
        }
        if !parser.is_live() || !parser.parse_literal(b"{") {
            return None;
        }
        loop {
            let token = parser.parse_ext(true);
            if token.is_empty() || token.eq_ignore_ascii_case(b"}") {
                return None;
            }
            if token.eq_ignore_ascii_case(key) {
                return Some(parser.parse_string().to_vec());
            }
            parser.skip_rest_of_line();
        }
    }

    /// `WP_SaberValidForPlayerInMP`: not marked `notInMP` (with a non-zero number).
    pub fn valid_for_player_in_mp(&self, name: &[u8]) -> bool {
        match self.parse_parm(name, b"notInMP") {
            Some(value) if !value.is_empty() => crate::userinfo::atoi(&value) == 0,
            _ => true,
        }
    }
}

/// `WP_SetSaber` for a client (`entNum < MAX_CLIENTS`): hand `hand` of `sabers` set to
/// the saber called `name`. `none` or `remove` takes the second saber away and leaves
/// the first; a campaign-only saber is the default one; a two-handed saber leaves no
/// second.
pub fn set_saber(
    parms: &SaberParms,
    sabers: &mut [SaberDefinition; 2],
    hand: usize,
    name: &[u8],
    host: &mut impl SaberParseHost,
) -> Result<(), SaberDefinitionError> {
    if name.eq_ignore_ascii_case(b"none") || name.eq_ignore_ascii_case(b"remove") {
        if hand != 0 {
            sabers[hand] = SaberDefinition::removed(host);
        }
        return Ok(());
    }
    let name = if parms.valid_for_player_in_mp(name) {
        name
    } else {
        DEFAULT_SABER
    };
    sabers[hand] = parms.parse(name, host)?.1;
    if sabers[1].flags & SFL_TWO_HANDED != 0
        || (sabers[0].flags & SFL_TWO_HANDED != 0 && sabers[1].is_held())
    {
        sabers[1] = SaberDefinition::removed(host);
    }
    Ok(())
}

/// Which of a player's sabers are lit, as the style rules read `saberHolstered`: both of
/// a pair unless holstered, the first alone at one; a staff unless fully holstered; a
/// single unless holstered at all.
fn active(sabers: &[SaberDefinition; 2], holstered: i32) -> (bool, bool) {
    if sabers[1].is_held() {
        return (holstered <= 1, holstered <= 0);
    }
    let first = sabers[0].is_held()
        && if sabers[0].num_blades > 1 {
            holstered <= 1
        } else {
            holstered == 0
        };
    (first, false)
}

/// `WP_SaberStyleValidForSaber`: whether `style` may be used with these sabers. A pair
/// allows the dual style, and Tavion's where both sabers teach it.
pub fn style_valid(sabers: &[SaberDefinition; 2], holstered: i32, style: i32) -> bool {
    let (first_active, second_active) = active(sabers, holstered);
    let forbids = |saber: &SaberDefinition| saber.styles_forbidden & (1 << style) != 0;
    if first_active && sabers[0].is_held() && forbids(&sabers[0]) {
        return false;
    }
    if sabers[1].is_held() && second_active {
        if forbids(&sabers[1]) {
            return false;
        }
        if style != SS_DUAL
            && (style != SS_TAVION
                || !(first_active && sabers[0].styles_learned & (1 << SS_TAVION) != 0)
                || sabers[1].styles_learned & (1 << SS_TAVION) == 0)
        {
            return false;
        }
    }
    true
}

/// `WP_UseFirstValidSaberStyle`: `style` moved to the first style both active sabers
/// allow, where the one it is forbidden. Whether it moved.
pub fn use_first_valid_style(
    sabers: &[SaberDefinition; 2],
    holstered: i32,
    style: &mut i32,
) -> bool {
    let (first_active, second_active) = active(sabers, holstered);
    let mut invalid = false;
    let mut valid = (1 << SS_NUM_SABER_STYLES) - 2;
    if first_active
        && sabers[0].is_held()
        && sabers[0].styles_forbidden != 0
        && sabers[0].styles_forbidden & (1 << *style) != 0
    {
        invalid = true;
        valid &= !sabers[0].styles_forbidden;
    }
    if sabers[1].is_held()
        && second_active
        && sabers[1].styles_forbidden != 0
        && sabers[1].styles_forbidden & (1 << *style) != 0
    {
        invalid = true;
        valid &= !sabers[1].styles_forbidden;
    }
    if valid == 0 || !invalid {
        // With no style left the reference only warns.
        return false;
    }
    match (SS_FAST..SS_NUM_SABER_STYLES).find(|candidate| valid & (1 << candidate) != 0) {
        Some(found) => {
            *style = found;
            true
        }
        None => false,
    }
}
