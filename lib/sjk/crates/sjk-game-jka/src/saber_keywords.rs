//! `saberParseKeywords` (`codemp/game/bg_saberLoad.c:1849-2003`): every keyword a saber
//! block may hold, and what the game module does with it. Each row names its kind of
//! value; [`Keyword::apply`] reads it as the keyword's `Saber_Parse*` function does,
//! failures included (a number missing from its line skips the next line).
//!
//! What only the client uses (shaders, effects) is read and dropped, as the game module
//! does; the sounds are registered, since the game module registers them too.

use crate::legacy_animation;
use crate::saber_definition::{
    MAX_BLADES, MAX_QPATH, SABER_NAME_LENGTH, SaberDefinition, SaberDefinitionError,
    SaberParseHost, truncated,
};
use crate::text_parse::TextParser;

/// How a keyword's value is read and where it goes.
#[derive(Clone, Copy)]
pub(crate) enum Keyword {
    /// `name`: the proper name, a string.
    FullName,
    /// `saberType`: a `saberTable` name.
    SaberType,
    /// `saberModel`: the hilt.
    Model,
    /// `customSkin`.
    Skin,
    /// A sound, registered.
    Sound(fn(&mut SaberDefinition) -> &mut u16),
    /// `numBlades`, one to [`MAX_BLADES`].
    NumBlades,
    /// A colour, length or radius for every blade (`None`) or one.
    Color(Option<usize>),
    Length(Option<usize>),
    Radius(Option<usize>),
    /// `saberStyle`: the one style it teaches, all others forbidden.
    Style,
    /// `saberStyleLearned`, `saberStyleForbidden`: one more style taught, forbidden.
    StyleLearned,
    StyleForbidden,
    /// `singleBladeStyle`.
    SingleBladeStyle,
    /// A whole number, a line skipped without one.
    Int(fn(&mut SaberDefinition) -> &mut i32),
    /// A number, a line skipped without one.
    Float(fn(&mut SaberDefinition) -> &mut f32),
    /// A `saberFlags` bit set by a zero (`lockable 0`), or by anything else.
    FlagWhenZero(u32),
    Flag(u32),
    /// A `saberFlags2` bit set by anything but zero.
    Flag2(u32),
    /// `forceRestrict`: a Force power by name.
    ForceRestrict,
    /// One of [`SaberDefinition::special_moves`] by a `saberMoveTable` name.
    Move(usize),
    /// One of [`SaberDefinition::anims`] by an animation name.
    Anim(usize),
    /// Read and kept nowhere (`brokenSaber1`, `brokenSaber2`).
    Ignored,
    /// The rest of the line skipped (`onInWater`, `notInMP`).
    RestOfLine,
    /// The client's alone: a string read, the rest of the line skipped.
    ClientOnly,
}

use Keyword::*;

/// The table, in the reference's order.
static KEYWORDS: [(&str, Keyword); 148] = [
    ("name", FullName),
    ("saberType", SaberType),
    ("saberModel", Model),
    ("customSkin", Skin),
    ("soundOn", Sound(|saber| &mut saber.sound_on)),
    ("soundLoop", Sound(|saber| &mut saber.sound_loop)),
    ("soundOff", Sound(|saber| &mut saber.sound_off)),
    ("numBlades", NumBlades),
    ("saberColor", Color(None)),
    ("saberColor2", Color(Some(1))),
    ("saberColor3", Color(Some(2))),
    ("saberColor4", Color(Some(3))),
    ("saberColor5", Color(Some(4))),
    ("saberColor6", Color(Some(5))),
    ("saberColor7", Color(Some(6))),
    ("saberLength", Length(None)),
    ("saberLength2", Length(Some(1))),
    ("saberLength3", Length(Some(2))),
    ("saberLength4", Length(Some(3))),
    ("saberLength5", Length(Some(4))),
    ("saberLength6", Length(Some(5))),
    ("saberLength7", Length(Some(6))),
    ("saberRadius", Radius(None)),
    ("saberRadius2", Radius(Some(1))),
    ("saberRadius3", Radius(Some(2))),
    ("saberRadius4", Radius(Some(3))),
    ("saberRadius5", Radius(Some(4))),
    ("saberRadius6", Radius(Some(5))),
    ("saberRadius7", Radius(Some(6))),
    ("saberStyle", Style),
    ("saberStyleLearned", StyleLearned),
    ("saberStyleForbidden", StyleForbidden),
    ("maxChain", Int(|saber| &mut saber.max_chain)),
    ("lockable", FlagWhenZero(1 << 0)),
    ("throwable", FlagWhenZero(1 << 1)),
    ("disarmable", FlagWhenZero(1 << 2)),
    ("blocking", FlagWhenZero(1 << 3)),
    ("twoHanded", Flag(1 << 4)),
    ("forceRestrict", ForceRestrict),
    ("lockBonus", Int(|saber| &mut saber.lock_bonus)),
    ("parryBonus", Int(|saber| &mut saber.parry_bonus)),
    (
        "breakParryBonus",
        Int(|saber| &mut saber.break_parry_bonus[0]),
    ),
    (
        "breakParryBonus2",
        Int(|saber| &mut saber.break_parry_bonus[1]),
    ),
    ("disarmBonus", Int(|saber| &mut saber.disarm_bonus[0])),
    ("disarmBonus2", Int(|saber| &mut saber.disarm_bonus[1])),
    ("singleBladeStyle", SingleBladeStyle),
    ("singleBladeThrowable", Flag(1 << 5)),
    ("brokenSaber1", Ignored),
    ("brokenSaber2", Ignored),
    ("returnDamage", Flag(1 << 6)),
    ("spinSound", Sound(|saber| &mut saber.spin_sound)),
    ("swingSound1", Sound(|saber| &mut saber.swing_sounds[0])),
    ("swingSound2", Sound(|saber| &mut saber.swing_sounds[1])),
    ("swingSound3", Sound(|saber| &mut saber.swing_sounds[2])),
    ("moveSpeedScale", Float(|saber| &mut saber.move_speed_scale)),
    ("animSpeedScale", Float(|saber| &mut saber.anim_speed_scale)),
    ("bounceOnWalls", Flag(1 << 8)),
    ("boltToWrist", Flag(1 << 9)),
    ("kataMove", Move(0)),
    ("lungeAtkMove", Move(1)),
    ("jumpAtkUpMove", Move(2)),
    ("jumpAtkFwdMove", Move(3)),
    ("jumpAtkBackMove", Move(4)),
    ("jumpAtkRightMove", Move(5)),
    ("jumpAtkLeftMove", Move(6)),
    ("readyAnim", Anim(0)),
    ("drawAnim", Anim(1)),
    ("putawayAnim", Anim(2)),
    ("tauntAnim", Anim(3)),
    ("bowAnim", Anim(4)),
    ("meditateAnim", Anim(5)),
    ("flourishAnim", Anim(6)),
    ("gloatAnim", Anim(7)),
    ("noRollStab", Flag(1 << 21)),
    ("noPullAttack", Flag(1 << 10)),
    ("noBackAttack", Flag(1 << 11)),
    ("noStabDown", Flag(1 << 12)),
    ("noWallRuns", Flag(1 << 13)),
    ("noWallFlips", Flag(1 << 14)),
    ("noWallGrab", Flag(1 << 15)),
    ("noRolls", Flag(1 << 16)),
    ("noFlips", Flag(1 << 17)),
    ("noCartwheels", Flag(1 << 18)),
    ("noKicks", Flag(1 << 19)),
    ("noMirrorAttacks", Flag(1 << 20)),
    ("onInWater", RestOfLine),
    ("notInMP", RestOfLine),
    (
        "bladeStyle2Start",
        Int(|saber| &mut saber.blade_style2_start),
    ),
    ("noWallMarks", Flag2(1 << 0)),
    ("noWallMarks2", Flag2(1 << 9)),
    ("noDlight", Flag2(1 << 1)),
    ("noDlight2", Flag2(1 << 10)),
    ("noBlade", Flag2(1 << 2)),
    ("noBlade2", Flag2(1 << 11)),
    ("trailStyle", Int(|saber| &mut saber.trail_style[0])),
    ("trailStyle2", Int(|saber| &mut saber.trail_style[1])),
    ("g2MarksShader", ClientOnly),
    ("g2MarksShader2", ClientOnly),
    ("g2WeaponMarkShader", ClientOnly),
    ("g2WeaponMarkShader2", ClientOnly),
    (
        "knockbackScale",
        Float(|saber| &mut saber.knockback_scale[0]),
    ),
    (
        "knockbackScale2",
        Float(|saber| &mut saber.knockback_scale[1]),
    ),
    ("damageScale", Float(|saber| &mut saber.damage_scale[0])),
    ("damageScale2", Float(|saber| &mut saber.damage_scale[1])),
    ("noDismemberment", Flag2(1 << 4)),
    ("noDismemberment2", Flag2(1 << 13)),
    ("noIdleEffect", Flag2(1 << 5)),
    ("noIdleEffect2", Flag2(1 << 14)),
    ("alwaysBlock", Flag2(1 << 6)),
    ("alwaysBlock2", Flag2(1 << 15)),
    ("noManualDeactivate", Flag2(1 << 7)),
    ("noManualDeactivate2", Flag2(1 << 16)),
    ("transitionDamage", Flag2(1 << 8)),
    ("transitionDamage2", Flag2(1 << 17)),
    ("splashRadius", Float(|saber| &mut saber.splash_radius[0])),
    ("splashRadius2", Float(|saber| &mut saber.splash_radius[1])),
    ("splashDamage", Int(|saber| &mut saber.splash_damage[0])),
    ("splashDamage2", Int(|saber| &mut saber.splash_damage[1])),
    (
        "splashKnockback",
        Float(|saber| &mut saber.splash_knockback[0]),
    ),
    (
        "splashKnockback2",
        Float(|saber| &mut saber.splash_knockback[1]),
    ),
    ("hitSound1", Sound(|saber| &mut saber.hit_sounds[0][0])),
    ("hit2Sound1", Sound(|saber| &mut saber.hit_sounds[1][0])),
    ("hitSound2", Sound(|saber| &mut saber.hit_sounds[0][1])),
    ("hit2Sound2", Sound(|saber| &mut saber.hit_sounds[1][1])),
    ("hitSound3", Sound(|saber| &mut saber.hit_sounds[0][2])),
    ("hit2Sound3", Sound(|saber| &mut saber.hit_sounds[1][2])),
    ("blockSound1", Sound(|saber| &mut saber.block_sounds[0][0])),
    ("block2Sound1", Sound(|saber| &mut saber.block_sounds[1][0])),
    ("blockSound2", Sound(|saber| &mut saber.block_sounds[0][1])),
    ("block2Sound2", Sound(|saber| &mut saber.block_sounds[1][1])),
    ("blockSound3", Sound(|saber| &mut saber.block_sounds[0][2])),
    ("block2Sound3", Sound(|saber| &mut saber.block_sounds[1][2])),
    (
        "bounceSound1",
        Sound(|saber| &mut saber.bounce_sounds[0][0]),
    ),
    (
        "bounce2Sound1",
        Sound(|saber| &mut saber.bounce_sounds[1][0]),
    ),
    (
        "bounceSound2",
        Sound(|saber| &mut saber.bounce_sounds[0][1]),
    ),
    (
        "bounce2Sound2",
        Sound(|saber| &mut saber.bounce_sounds[1][1]),
    ),
    (
        "bounceSound3",
        Sound(|saber| &mut saber.bounce_sounds[0][2]),
    ),
    (
        "bounce2Sound3",
        Sound(|saber| &mut saber.bounce_sounds[1][2]),
    ),
    ("blockEffect", ClientOnly),
    ("blockEffect2", ClientOnly),
    ("hitPersonEffect", ClientOnly),
    ("hitPersonEffect2", ClientOnly),
    ("hitOtherEffect", ClientOnly),
    ("hitOtherEffect2", ClientOnly),
    ("bladeEffect", ClientOnly),
    ("bladeEffect2", ClientOnly),
    ("noClashFlare", Flag2(1 << 3)),
    ("noClashFlare2", Flag2(1 << 12)),
];

/// `saberTable`: the types a `saberType` may name, by value (`SABER_SITH_SWORD` is not
/// among them).
const SABER_TYPES: [(&str, i32); 12] = [
    ("SABER_NONE", 0),
    ("SABER_SINGLE", 1),
    ("SABER_STAFF", 2),
    ("SABER_BROAD", 4),
    ("SABER_PRONG", 5),
    ("SABER_DAGGER", 3),
    ("SABER_ARC", 6),
    ("SABER_SAI", 7),
    ("SABER_CLAW", 8),
    ("SABER_LANCE", 9),
    ("SABER_STAR", 10),
    ("SABER_TRIDENT", 11),
];
/// `NUM_SABERS`.
const NUM_SABERS: i32 = 13;

/// `saberMoveTable`: `LS_NONE`, then the moves from `LS_A_TL2BR` (4) to `LS_HILT_BASH`
/// (61) in their enum order.
const SABER_MOVES: [&str; 58] = [
    "LS_A_TL2BR",
    "LS_A_L2R",
    "LS_A_BL2TR",
    "LS_A_BR2TL",
    "LS_A_R2L",
    "LS_A_TR2BL",
    "LS_A_T2B",
    "LS_A_BACKSTAB",
    "LS_A_BACK",
    "LS_A_BACK_CR",
    "LS_ROLL_STAB",
    "LS_A_LUNGE",
    "LS_A_JUMP_T__B_",
    "LS_A_FLIP_STAB",
    "LS_A_FLIP_SLASH",
    "LS_JUMPATTACK_DUAL",
    "LS_JUMPATTACK_ARIAL_LEFT",
    "LS_JUMPATTACK_ARIAL_RIGHT",
    "LS_JUMPATTACK_CART_LEFT",
    "LS_JUMPATTACK_CART_RIGHT",
    "LS_JUMPATTACK_STAFF_LEFT",
    "LS_JUMPATTACK_STAFF_RIGHT",
    "LS_BUTTERFLY_LEFT",
    "LS_BUTTERFLY_RIGHT",
    "LS_A_BACKFLIP_ATK",
    "LS_SPINATTACK_DUAL",
    "LS_SPINATTACK",
    "LS_LEAP_ATTACK",
    "LS_SWOOP_ATTACK_RIGHT",
    "LS_SWOOP_ATTACK_LEFT",
    "LS_TAUNTAUN_ATTACK_RIGHT",
    "LS_TAUNTAUN_ATTACK_LEFT",
    "LS_KICK_F",
    "LS_KICK_B",
    "LS_KICK_R",
    "LS_KICK_L",
    "LS_KICK_S",
    "LS_KICK_BF",
    "LS_KICK_RL",
    "LS_KICK_F_AIR",
    "LS_KICK_B_AIR",
    "LS_KICK_R_AIR",
    "LS_KICK_L_AIR",
    "LS_STABDOWN",
    "LS_STABDOWN_STAFF",
    "LS_STABDOWN_DUAL",
    "LS_DUAL_SPIN_PROTECT",
    "LS_STAFF_SOULCAL",
    "LS_A1_SPECIAL",
    "LS_A2_SPECIAL",
    "LS_A3_SPECIAL",
    "LS_UPSIDE_DOWN_ATTACK",
    "LS_PULL_ATTACK_STAB",
    "LS_PULL_ATTACK_SWING",
    "LS_SPINATTACK_ALORA",
    "LS_DUAL_FB",
    "LS_DUAL_LR",
    "LS_HILT_BASH",
];
/// The value of [`SABER_MOVES`]' first.
const FIRST_ATTACK: i32 = 4;

/// `FPTable` (`bg_saga.c:115-136`): the Force powers by name, in their enum's order.
pub(crate) const FORCE_POWERS: [&str; 18] = [
    "FP_HEAL",
    "FP_LEVITATION",
    "FP_SPEED",
    "FP_PUSH",
    "FP_PULL",
    "FP_TELEPATHY",
    "FP_GRIP",
    "FP_LIGHTNING",
    "FP_RAGE",
    "FP_PROTECT",
    "FP_ABSORB",
    "FP_TEAM_HEAL",
    "FP_TEAM_FORCE",
    "FP_DRAIN",
    "FP_SEE",
    "FP_SABER_OFFENSE",
    "FP_SABER_DEFENSE",
    "FP_SABERTHROW",
];

/// `KeywordHash_Find`: the keyword `token` names, any case.
pub(crate) fn find(token: &[u8]) -> Option<Keyword> {
    KEYWORDS
        .iter()
        .find(|(name, _)| name.as_bytes().eq_ignore_ascii_case(token))
        .map(|(_, keyword)| *keyword)
}

/// `TranslateSaberColor`: a colour by name; `random` draws one from orange to purple;
/// anything else is blue.
pub(crate) fn color(name: &[u8], host: &mut impl SaberParseHost) -> i32 {
    const COLORS: [&str; 6] = ["red", "orange", "yellow", "green", "blue", "purple"];
    if name.eq_ignore_ascii_case(b"random") {
        return host.irand(1, 5);
    }
    COLORS
        .iter()
        .position(|color| color.as_bytes().eq_ignore_ascii_case(name))
        .map_or(4, |index| index as i32)
}

/// `TranslateSaberStyle`: a style by name, `SS_NONE` for anything else.
fn style(name: &[u8]) -> i32 {
    const STYLES: [&str; 7] = [
        "fast", "medium", "strong", "desann", "tavion", "dual", "staff",
    ];
    STYLES
        .iter()
        .position(|style| style.as_bytes().eq_ignore_ascii_case(name))
        .map_or(0, |index| index as i32 + 1)
}

/// `GetIDForString` over names whose values run from `first`.
fn id_of(names: &[&str], first: i32, name: &[u8]) -> Option<i32> {
    names
        .iter()
        .position(|known| known.as_bytes().eq_ignore_ascii_case(name))
        .map(|index| index as i32 + first)
}

/// The blades a colour, length or radius goes to.
fn blades(which: Option<usize>) -> std::ops::Range<usize> {
    which.map_or(0..MAX_BLADES, |blade| blade..blade + 1)
}

impl Keyword {
    /// Reads this keyword's value from `parser` into `saber`.
    pub(crate) fn apply(
        self,
        saber: &mut SaberDefinition,
        parser: &mut TextParser<'_>,
        host: &mut impl SaberParseHost,
    ) -> Result<(), SaberDefinitionError> {
        // A number read by `COM_ParseInt`/`COM_ParseFloat`: without one, the rest of
        // the line goes.
        let int = |parser: &mut TextParser<'_>| {
            let value = parser.parse_int();
            if value.is_none() {
                parser.skip_rest_of_line();
            }
            value
        };
        match self {
            FullName => saber.full_name = truncated(parser.parse_string(), SABER_NAME_LENGTH),
            SaberType => {
                let name = parser.parse_string();
                let value = SABER_TYPES
                    .iter()
                    .find(|(known, _)| known.as_bytes().eq_ignore_ascii_case(name))
                    .map(|(_, value)| *value);
                if let Some(value) = value.filter(|value| (1..NUM_SABERS).contains(value)) {
                    saber.saber_type = value;
                }
            }
            Model => saber.model = truncated(parser.parse_string(), MAX_QPATH),
            Skin => saber.skin = parser.parse_string().to_vec(),
            Sound(field) => *field(saber) = host.sound_index(parser.parse_string()),
            NumBlades => {
                if let Some(blades) = int(parser) {
                    if !(1..=MAX_BLADES as i32).contains(&blades) {
                        return Err(SaberDefinitionError::IllegalBlades {
                            saber: saber.name.clone(),
                            blades,
                        });
                    }
                    saber.num_blades = blades;
                }
            }
            Color(which) => {
                let value = color(parser.parse_string(), host);
                blades(which).for_each(|blade| saber.blades[blade].color = value);
            }
            Length(which) => {
                if let Some(value) = parser.parse_float() {
                    blades(which).for_each(|blade| saber.blades[blade].length_max = value.max(4.0));
                }
            }
            Radius(which) => {
                if let Some(value) = parser.parse_float() {
                    blades(which).for_each(|blade| saber.blades[blade].radius = value.max(0.25));
                }
            }
            Style => {
                let only = style(parser.parse_string());
                saber.styles_learned = 1 << only;
                saber.styles_forbidden = (1..8)
                    .filter(|other| *other != only)
                    .fold(0, |bits, other| bits | 1 << other);
            }
            StyleLearned => saber.styles_learned |= 1 << style(parser.parse_string()),
            StyleForbidden => saber.styles_forbidden |= 1 << style(parser.parse_string()),
            SingleBladeStyle => saber.single_blade_style = style(parser.parse_string()),
            Int(field) => {
                if let Some(value) = int(parser) {
                    *field(saber) = value;
                }
            }
            Float(field) => match parser.parse_float() {
                Some(value) => *field(saber) = value,
                None => parser.skip_rest_of_line(),
            },
            FlagWhenZero(bit) => {
                if int(parser) == Some(0) {
                    saber.flags |= bit;
                }
            }
            Flag(bit) => {
                if int(parser).is_some_and(|value| value != 0) {
                    saber.flags |= bit;
                }
            }
            Flag2(bit) => {
                if int(parser).is_some_and(|value| value != 0) {
                    saber.flags2 |= bit;
                }
            }
            ForceRestrict => {
                if let Some(power) = id_of(&FORCE_POWERS, 0, parser.parse_string()) {
                    saber.force_restrictions |= 1 << power;
                }
            }
            Move(slot) => {
                // An unknown name reads as `LS_INVALID`, which the range lets through.
                let name = parser.parse_string();
                let value = if name.eq_ignore_ascii_case(b"LS_NONE") {
                    Some(0)
                } else {
                    id_of(&SABER_MOVES, FIRST_ATTACK, name)
                };
                saber.special_moves[slot] = value.unwrap_or(crate::saber_info::LS_INVALID);
            }
            Anim(slot) => {
                if let Some(value) = id_of(legacy_animation::NAMES, 0, parser.parse_string()) {
                    saber.anims[slot] = value;
                }
            }
            Ignored => {
                parser.parse_string();
            }
            RestOfLine => parser.skip_rest_of_line(),
            ClientOnly => {
                parser.parse_string();
                parser.skip_rest_of_line();
            }
        }
        Ok(())
    }
}
