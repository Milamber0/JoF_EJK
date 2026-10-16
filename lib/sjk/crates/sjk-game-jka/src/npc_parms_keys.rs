//! The keys of an NPC's block (`NPC_ParseParms`, `NPC_stats.c:1152-3505`), by group:
//! model, look clamps and scale; AI stats; team, class and health; bounds and movement;
//! sounds; weapons and the Force. The sabers' keys are in [`crate::npc_parms_sabers`].
//!
//! Each key reads its value as the reference does, and fails as it does:
//! - a number missing from its line (`COM_ParseInt`/`COM_ParseFloat` failing) skips the
//!   rest of the line, which is the whole next line, except where the reference only
//!   `continue`s (`width`, `height`, `crouchheight` and `customRGBA`'s numbers);
//! - a number out of range is dropped with the rest of its line left to be read as keys;
//! - a string (`COM_ParseString`) never fails: past the line's end it is empty.
//!
//! No key is refused for being the player's: multiplayer never parses entity 0.

use crate::npc_parms::{
    CLASS_VEHICLE, DEFAULT_MINS_2, MAX_QPATH, NpcDefinition, NpcRefusalReason, NpcScale, NpcSpawn,
    NpcVehicle, new_string,
};
use crate::saber_definition::{SaberParms, SaberParseHost, truncated};
use crate::text_parse::TextParser;

/// What became of a key.
pub(crate) enum KeyOutcome {
    /// One of the block's keys, read.
    Read,
    /// Not a key: the reference warns and skips the rest of the line.
    Unknown,
}

/// The parse in progress: the NPC being written, how it was spawned, the sabers it may
/// name, and whether it is still an MD3 model (no `playerModel`, not a vehicle).
pub(crate) struct Reader<'a> {
    pub npc: &'a mut NpcDefinition,
    pub spawn: &'a NpcSpawn,
    pub sabers: &'a SaberParms,
    pub md3_model: bool,
}

/// `ClassTable` (`NPC_stats.c:44-103`): `class_t` by name, in the enum's order.
const CLASSES: [&str; 56] = [
    "CLASS_NONE",
    "CLASS_ATST",
    "CLASS_BARTENDER",
    "CLASS_BESPIN_COP",
    "CLASS_CLAW",
    "CLASS_COMMANDO",
    "CLASS_DESANN",
    "CLASS_FISH",
    "CLASS_FLIER2",
    "CLASS_GALAK",
    "CLASS_GLIDER",
    "CLASS_GONK",
    "CLASS_GRAN",
    "CLASS_HOWLER",
    "CLASS_IMPERIAL",
    "CLASS_IMPWORKER",
    "CLASS_INTERROGATOR",
    "CLASS_JAN",
    "CLASS_JEDI",
    "CLASS_KYLE",
    "CLASS_LANDO",
    "CLASS_LIZARD",
    "CLASS_LUKE",
    "CLASS_MARK1",
    "CLASS_MARK2",
    "CLASS_GALAKMECH",
    "CLASS_MINEMONSTER",
    "CLASS_MONMOTHA",
    "CLASS_MORGANKATARN",
    "CLASS_MOUSE",
    "CLASS_MURJJ",
    "CLASS_PRISONER",
    "CLASS_PROBE",
    "CLASS_PROTOCOL",
    "CLASS_R2D2",
    "CLASS_R5D2",
    "CLASS_REBEL",
    "CLASS_REBORN",
    "CLASS_REELO",
    "CLASS_REMOTE",
    "CLASS_RODIAN",
    "CLASS_SEEKER",
    "CLASS_SENTRY",
    "CLASS_SHADOWTROOPER",
    "CLASS_STORMTROOPER",
    "CLASS_SWAMP",
    "CLASS_SWAMPTROOPER",
    "CLASS_TAVION",
    "CLASS_TRANDOSHAN",
    "CLASS_UGNAUGHT",
    "CLASS_JAWA",
    "CLASS_WEEQUAY",
    "CLASS_BOBAFETT",
    "CLASS_VEHICLE",
    "CLASS_RANCOR",
    "CLASS_WAMPA",
];

/// `TeamTable` (`NPC_stats.c:34-41`): `npcteam_t` by name, in the table's order.
const TEAMS: [(&str, i32); 4] = [
    ("NPCTEAM_FREE", 0),
    ("NPCTEAM_PLAYER", 2),
    ("NPCTEAM_ENEMY", 1),
    ("NPCTEAM_NEUTRAL", 3),
];

/// `TranslateRankName` (`NPC_stats.c:278-322`): `rank_t` by name, anything else a civilian.
const RANKS: [&str; 8] = [
    "civilian",
    "crewman",
    "ensign",
    "ltjg",
    "lt",
    "ltcomm",
    "commander",
    "captain",
];

/// `NUM_BSTATES`: the default behaviour must be below it.
const NUM_BSTATES: i32 = 17;
/// `Q_strcat`'s buffer for `surfOff` and `surfOn`.
const SURFACE_LIST_SIZE: usize = 1024;

/// `COM_ParseInt`, and on failure `SkipRestOfLine`.
pub(crate) fn int_or_skip(parser: &mut TextParser<'_>) -> Option<i32> {
    let value = parser.parse_int();
    if value.is_none() {
        parser.skip_rest_of_line();
    }
    value
}

/// `COM_ParseFloat`, and on failure `SkipRestOfLine`.
pub(crate) fn float_or_skip(parser: &mut TextParser<'_>) -> Option<f32> {
    let value = parser.parse_float();
    if value.is_none() {
        parser.skip_rest_of_line();
    }
    value
}

/// The position of `name` in `names`, any case (`GetIDForString` over a table numbered
/// in order), -1 for none.
fn position(names: &[&str], name: &[u8]) -> i32 {
    names
        .iter()
        .position(|known| known.as_bytes().eq_ignore_ascii_case(name))
        .map_or(-1, |index| index as i32)
}

impl Reader<'_> {
    /// One key and its value (`token` is the key, any case).
    pub(crate) fn key(
        &mut self,
        token: &[u8],
        parser: &mut TextParser<'_>,
        host: &mut impl SaberParseHost,
    ) -> Result<KeyOutcome, NpcRefusalReason> {
        let key = token.to_ascii_lowercase();
        let read = self.model_key(&key, parser, host)
            || self.stats_key(&key, parser)
            || self.identity_key(&key, parser)?
            || self.movement_key(&key, parser)
            || self.equipment_key(&key, parser)
            || crate::npc_parms_sabers::saber_key(self, &key, parser, host)?;
        Ok(if read {
            KeyOutcome::Read
        } else {
            KeyOutcome::Unknown
        })
    }

    /// The model, its colour, surfaces, look clamps and scale (`NPC_stats.c:1152-1804`).
    fn model_key(
        &mut self,
        key: &[u8],
        parser: &mut TextParser<'_>,
        host: &mut impl SaberParseHost,
    ) -> bool {
        let npc = &mut *self.npc;
        match key {
            b"customrgba" => custom_rgba(npc, parser, host),
            b"headmodel" | b"torsomodel" => {
                // "none" zeroes the part's clamps so the others do not lag behind it.
                if parser.parse_string().eq_ignore_ascii_case(b"none") {
                    let ranges = if key == b"headmodel" {
                        &mut npc.head_ranges
                    } else {
                        &mut npc.torso_ranges
                    };
                    *ranges = [0; 4];
                }
            }
            b"legsmodel" => {
                parser.parse_string();
            }
            b"playermodel" => {
                npc.player_model = truncated(parser.parse_string(), MAX_QPATH);
                self.md3_model = false;
            }
            b"customskin" => npc.custom_skin = truncated(parser.parse_string(), MAX_QPATH),
            b"surfoff" => surface_list(&mut npc.surf_off, parser.parse_string()),
            b"surfon" => surface_list(&mut npc.surf_on, parser.parse_string()),
            b"scale" => {
                if let Some(mut n) = int_or_skip(parser)
                    && n >= 0
                    && n != 100
                {
                    let percent = n;
                    // "MP does not support scaling up to or over 1024%".
                    if n >= 1024 {
                        n = 1023;
                    }
                    npc.scale = Some(NpcScale {
                        percent,
                        scale: n as f32 / 100.0,
                    });
                }
            }
            // Read and dropped: "MP doesn't support xyz scaling, use 'scale'".
            b"scalex" | b"scaley" | b"scalez" => {
                int_or_skip(parser);
            }
            _ => {
                let Some((ranges, at)) = look_range(npc, key) else {
                    return false;
                };
                if let Some(n) = int_or_skip(parser)
                    && n >= 0
                {
                    ranges[at] = n;
                }
            }
        }
        true
    }

    /// The AI stats and the rank (`NPC_stats.c:1807-2084`).
    fn stats_key(&mut self, key: &[u8], parser: &mut TextParser<'_>) -> bool {
        let stats = &mut self.npc.stats;
        let one_to_five =
            |parser: &mut TextParser<'_>| int_or_skip(parser).filter(|n| (1..=5).contains(n));
        let not_negative =
            |parser: &mut TextParser<'_>| float_or_skip(parser).filter(|f| *f >= 0.0);
        let field_of_view =
            |parser: &mut TextParser<'_>| int_or_skip(parser).filter(|n| (30..=180).contains(n));
        match key {
            b"aggression" => one_to_five(parser)
                .into_iter()
                .for_each(|n| stats.aggression = n),
            b"aim" => one_to_five(parser).into_iter().for_each(|n| stats.aim = n),
            b"evasion" => one_to_five(parser)
                .into_iter()
                .for_each(|n| stats.evasion = n),
            b"intelligence" => one_to_five(parser)
                .into_iter()
                .for_each(|n| stats.intelligence = n),
            b"move" => one_to_five(parser)
                .into_iter()
                .for_each(|n| stats.movement = n),
            b"reactions" => one_to_five(parser)
                .into_iter()
                .for_each(|n| stats.reactions = n),
            b"earshot" => not_negative(parser)
                .into_iter()
                .for_each(|f| stats.earshot = f),
            b"shootdistance" => not_negative(parser)
                .into_iter()
                .for_each(|f| stats.shoot_distance = f),
            b"vigilance" => not_negative(parser)
                .into_iter()
                .for_each(|f| stats.vigilance = f),
            b"visrange" => not_negative(parser)
                .into_iter()
                .for_each(|f| stats.visrange = f),
            b"hfov" => field_of_view(parser)
                .into_iter()
                .for_each(|n| stats.hfov = n),
            // Halved, where `hfov` is not.
            b"vfov" => field_of_view(parser)
                .into_iter()
                .for_each(|n| stats.vfov = n / 2),
            b"rank" => self.npc.rank = position(&RANKS, parser.parse_string()).max(0),
            _ => return false,
        }
        true
    }

    /// Health, name, teams, class and the dropped dismemberment odds
    /// (`NPC_stats.c:2087-2267`). A vehicle class on an NPC not spawned as one refuses it.
    fn identity_key(
        &mut self,
        key: &[u8],
        parser: &mut TextParser<'_>,
    ) -> Result<bool, NpcRefusalReason> {
        let npc = &mut *self.npc;
        match key {
            b"health" => {
                if let Some(n) = int_or_skip(parser)
                    && n >= 0
                {
                    npc.stats.health = n;
                }
            }
            b"fullname" => npc.full_name = Some(new_string(parser.parse_string())),
            // `token` and `value` are the same buffer: the name looked up is "NPC" and the
            // value ("TEAM_ENEMY" is `NPCTEAM_ENEMY`), not the key.
            b"playerteam" => npc.player_team = Some(team(parser.parse_string())),
            b"enemyteam" => npc.enemy_team = Some(team(parser.parse_string())),
            b"class" => {
                let class = position(&CLASSES, parser.parse_string());
                npc.client_class = class;
                npc.entity_class = class;
                if class == CLASS_VEHICLE {
                    if self.spawn.vehicle.is_none() {
                        return Err(NpcRefusalReason::VehicleWithoutVehicle);
                    }
                    self.md3_model = false;
                }
            }
            b"dismemberprobhead"
            | b"dismemberprobarms"
            | b"dismemberprobhands"
            | b"dismemberprobwaist"
            | b"dismemberproblegs" => {
                int_or_skip(parser);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// The box, the heights, movement type and speeds, and the default behaviour
    /// (`NPC_stats.c:2270-2454`).
    fn movement_key(&mut self, key: &[u8], parser: &mut TextParser<'_>) -> bool {
        let npc = &mut *self.npc;
        let not_negative = |parser: &mut TextParser<'_>| int_or_skip(parser).filter(|n| *n >= 0);
        match key {
            b"width" => {
                if let Some(n) = parser.parse_int() {
                    npc.mins[0] = -n as f32;
                    npc.mins[1] = -n as f32;
                    npc.maxs[0] = n as f32;
                    npc.maxs[1] = n as f32;
                }
            }
            b"height" => {
                if let Some(n) = parser.parse_int() {
                    height(npc, self.spawn.vehicle, n);
                }
            }
            b"crouchheight" => {
                if let Some(n) = parser.parse_int() {
                    npc.crouch_height = n + DEFAULT_MINS_2;
                }
            }
            b"movetype" => {
                if parser.parse_string().eq_ignore_ascii_case(b"flyswim") {
                    npc.flying = true;
                }
            }
            b"yawspeed" => int_or_skip(parser)
                .filter(|n| *n > 0)
                .into_iter()
                .for_each(|n| npc.stats.yaw_speed = n as f32),
            b"walkspeed" => not_negative(parser)
                .into_iter()
                .for_each(|n| npc.stats.walk_speed = n),
            b"runspeed" => not_negative(parser)
                .into_iter()
                .for_each(|n| npc.stats.run_speed = n),
            b"acceleration" => not_negative(parser)
                .into_iter()
                .for_each(|n| npc.stats.acceleration = n),
            // "sex - skip in MP".
            b"sex" => parser.skip_rest_of_line(),
            b"behavior" => int_or_skip(parser)
                .filter(|n| (0..NUM_BSTATES).contains(n))
                .into_iter()
                .for_each(|n| npc.default_behavior = n),
            _ => return false,
        }
        true
    }

    /// Sound sets (read, and registered only by the precache), weapons, alt-fire and the
    /// Force (`NPC_stats.c:2457-2644`).
    fn equipment_key(&mut self, key: &[u8], parser: &mut TextParser<'_>) -> bool {
        let npc = &mut *self.npc;
        match key {
            b"snd" | b"sndcombat" | b"sndextra" | b"sndjedi" => {
                parser.parse_string();
            }
            b"weapon" => {
                if let Some(weapon) = crate::weapon_data::weapon_by_name(parser.parse_string()) {
                    npc.weapon = Some(weapon);
                    npc.weapons |= 1 << weapon;
                    if weapon > 0 {
                        npc.ammo_filled |=
                            1 << crate::weapon_data::LEGACY_WEAPON_DATA[weapon as usize].ammo_index;
                    }
                }
            }
            b"altfire" => {
                if int_or_skip(parser).is_some_and(|n| n != 0) {
                    npc.alt_fire = true;
                }
            }
            b"forcepowermax" => npc.force_power_max = int_or_skip(parser).or(npc.force_power_max),
            // "rwwFIXMEFIXME: support this?": read and dropped.
            b"forceregenrate" | b"forceregenamount" => {
                int_or_skip(parser);
            }
            _ => {
                let power = position(&crate::saber_keywords::FORCE_POWERS, key);
                if power < 0 {
                    return false;
                }
                if let Some(n) = int_or_skip(parser) {
                    npc.force_levels[power as usize] = Some(n.clamp(0, 5));
                }
            }
        }
        true
    }
}

/// `customRGBA` (`NPC_stats.c:1152-1517`): `random`, one of the named palettes drawn
/// from, or up to four numbers (the first by `atoi` of whatever word is there).
fn custom_rgba(
    npc: &mut NpcDefinition,
    parser: &mut TextParser<'_>,
    host: &mut impl SaberParseHost,
) {
    let value = parser.parse_string().to_ascii_lowercase();
    let rgba = &mut npc.custom_rgba;
    if value == b"random" {
        for channel in rgba.iter_mut().take(3) {
            *channel = host.irand(0, 255);
        }
        rgba[3] = 255;
        return;
    }
    if let Some(palette) = palette(&value) {
        rgba[3] = 255;
        let drawn = host.irand(0, palette.len() as i32 - 1);
        rgba[..3].copy_from_slice(&palette[drawn as usize]);
        return;
    }
    rgba[0] = crate::userinfo::atoi(&value);
    for channel in &mut rgba[1..] {
        match parser.parse_int() {
            Some(n) => *channel = n,
            None => return,
        }
    }
}

/// `Q_strcat` of `,` and `value` onto a surface list, or the value alone into an empty
/// one (`NPC_stats.c:1583-1619`), in a buffer of [`SURFACE_LIST_SIZE`].
fn surface_list(list: &mut Vec<u8>, value: &[u8]) {
    if list.is_empty() {
        *list = truncated(value, SURFACE_LIST_SIZE);
    } else {
        for part in [&b","[..], value] {
            let room = (SURFACE_LIST_SIZE - 1).saturating_sub(list.len());
            list.extend_from_slice(&part[..part.len().min(room)]);
        }
    }
}

/// The look clamp a key names: `head` or `torso`, then `YawRangeLeft`, `YawRangeRight`,
/// `PitchRangeUp`, `PitchRangeDown`.
fn look_range<'a>(npc: &'a mut NpcDefinition, key: &[u8]) -> Option<(&'a mut [i32; 4], usize)> {
    const PARTS: [&[u8]; 4] = [
        b"yawrangeleft",
        b"yawrangeright",
        b"pitchrangeup",
        b"pitchrangedown",
    ];
    let (ranges, rest) = if let Some(rest) = key.strip_prefix(b"head") {
        (&mut npc.head_ranges, rest)
    } else {
        (&mut npc.torso_ranges, key.strip_prefix(b"torso")?)
    };
    PARTS
        .iter()
        .position(|part| *part == rest)
        .map(|at| (ranges, at))
}

/// `GetIDForString(TeamTable, "NPC" + value)`.
pub(crate) fn team(value: &[u8]) -> i32 {
    let name = [b"NPC".as_slice(), value].concat();
    TEAMS
        .iter()
        .find(|(known, _)| known.as_bytes().eq_ignore_ascii_case(&name))
        .map_or(-1, |(_, team)| *team)
}

/// `height` (`NPC_stats.c:2281-2318`): a fighter's box is centred on its origin (half the
/// height, as the whole number `standheight` keeps) and the origin is lifted to stand it
/// on the ground; anything else keeps its feet at `DEFAULT_MINS_2`. Either way the
/// height is the radius.
fn height(npc: &mut NpcDefinition, vehicle: Option<NpcVehicle>, n: i32) {
    if npc.client_class == CLASS_VEHICLE && vehicle == Some(NpcVehicle::Fighter) {
        // `maxs[2] = standheight = n / 2.0f`: the float stored in an int, and that int
        // read back as the float.
        npc.stand_height = (n as f32 / 2.0) as i32;
        npc.maxs[2] = npc.stand_height as f32;
        npc.mins[2] = -npc.maxs[2];
        npc.origin[2] += (DEFAULT_MINS_2 as f32 - npc.mins[2]) + 0.125;
    } else {
        npc.mins[2] = DEFAULT_MINS_2 as f32;
        npc.stand_height = n + DEFAULT_MINS_2;
        npc.maxs[2] = npc.stand_height as f32;
    }
    npc.radius = Some(n as f32);
}

/// The named skin-tone palettes `customRGBA` draws one colour from (`NPC_stats.c:
/// 1164-1497`), each drawn with `Q_irand(0, count - 1)`.
fn palette(name: &[u8]) -> Option<&'static [[i32; 3]]> {
    const RANDOM1: [[i32; 3]; 6] = [
        [127, 153, 255],
        [177, 29, 13],
        [47, 90, 40],
        [181, 207, 255],
        [138, 83, 0],
        [254, 199, 14],
    ];
    const JEDI_HF: [[i32; 3]; 8] = [
        [165, 48, 21],
        [254, 230, 132],
        [181, 207, 255],
        [233, 183, 208],
        [161, 226, 240],
        [101, 159, 255],
        [255, 157, 114],
        [216, 160, 255],
    ];
    const JEDI_HM: [[i32; 3]; 8] = [
        [252, 243, 180],
        [69, 109, 255],
        [254, 197, 73],
        [178, 78, 18],
        [112, 153, 161],
        [123, 182, 255],
        [0, 88, 105],
        [138, 0, 0],
    ];
    const JEDI_KDM: [[i32; 3]; 9] = [
        [85, 120, 255],
        [173, 142, 219],
        [254, 197, 73],
        [138, 83, 0],
        [254, 199, 14],
        [68, 194, 217],
        [170, 3, 30],
        [225, 226, 144],
        [167, 202, 255],
    ];
    const JEDI_RM: [[i32; 3]; 9] = [
        [127, 153, 255],
        [208, 249, 85],
        [181, 207, 255],
        [138, 83, 0],
        [224, 171, 44],
        [49, 155, 131],
        [163, 79, 17],
        [148, 104, 228],
        [138, 136, 0],
    ];
    const JEDI_TF: [[i32; 3]; 6] = [
        [255, 235, 100],
        [62, 155, 255],
        [255, 110, 120],
        [180, 150, 255],
        [255, 200, 212],
        [255, 255, 255],
    ];
    const JEDI_ZF: [[i32; 3]; 8] = [
        [204, 19, 21],
        [255, 107, 40],
        [255, 148, 155],
        [255, 164, 59],
        [216, 160, 255],
        [101, 159, 255],
        [161, 226, 240],
        [37, 155, 181],
    ];
    Some(match name {
        b"random1" => &RANDOM1,
        b"jedi_hf" => &JEDI_HF,
        b"jedi_hm" => &JEDI_HM,
        b"jedi_kdm" => &JEDI_KDM,
        b"jedi_rm" => &JEDI_RM,
        b"jedi_tf" => &JEDI_TF,
        b"jedi_zf" => &JEDI_ZF,
        _ => return None,
    })
}
