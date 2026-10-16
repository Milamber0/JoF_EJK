//! NPC definitions (`codemp/game/NPC_stats.c`): what `ext_data/npcs/*.npc` says an NPC
//! is, read the way the game reads it.
//!
//! - [`NpcParms`] is `NPCParms`, every file compressed and joined in the listing's order
//!   (`NPC_LoadParms`, `NPC_stats.c:3561-3613`). The listing is the one `FS_GetFileList`
//!   fits in its 2048-byte buffer: [`crate::saber_definition::listed_files`].
//! - [`NpcParms::parse`] is `NPC_ParseParms` (`NPC_stats.c:966-3558`) for an NPC being
//!   spawned: the defaults, the named block found, every key applied, and the end of the
//!   function (the default saber, the model the Ghoul2 setup registers and
//!   `NPC_Precache`'s registrations, `NPC_stats.c:589-859`).
//!
//! The result, [`NpcDefinition`], is a native record of everything the parse writes on
//! the entity, its client and its `gNPC_t`. Nothing is registered here: the names the
//! reference hands `G_ModelIndex`, `G_SoundIndex` and `RegisterItem` are listed in the
//! order it registers them, for the spawn to register. Only the sabers' own sounds go
//! through [`SaberParseHost`], as [`SaberParms::parse`] registers them; the random colours
//! draw from the same host, in the reference's order.
//!
//! Deliberately not here:
//! - the parse for the player (`NPC->s.number == 0`, `parsingPlayer`): multiplayer calls
//!   `NPC_ParseParms` only from `NPC_Spawn_Do`, never for entity 0;
//! - the Ghoul2 instance itself (`SetupGameGhoul2Model`: bolts, `localAnimIndex`, the
//!   saber instances). [`NpcDefinition::model`] is the configstring the setup registers
//!   *when the model loads*; a model that does not load registers nothing and falls back
//!   to Kyle's instance, which the spawn decides with the assets in hand;
//! - the team-game skin check `BG_ValidateSkinForTeam` in that setup: an NPC's session
//!   team is `TEAM_FREE`, and the setup reads it only for the player's no-skin path,
//!   which an NPC (whose skin is never empty) never takes;
//! - keys the multiplayer game reads and drops (`legsmodel`, `scaleX/Y/Z`,
//!   `dismemberProb*`, `forceRegenRate`, `forceRegenAmount`, `sex`, `snd*` on the parse's
//!   side): they are read exactly as the reference reads them, and change nothing.
//!
//! Held to `tools/game-oracle/npcparms.c` (`game-npcparms.txt`).

use crate::npc_parms_keys::KeyOutcome;
use crate::saber_definition::{
    DEFAULT_SABER, SaberDefinition, SaberDefinitionError, SaberParms, SaberParseHost,
};
use crate::text_parse::{TextParser, compress, until_nul};

/// `MAX_NPC_DATA_SIZE`: all the files together, compressed.
const MAX_NPC_DATA_SIZE: usize = 0x40000;
/// `MAX_QPATH`: the parse's name buffers keep one byte less.
pub(crate) const MAX_QPATH: usize = 64;
/// `DEFAULT_MINS_2`, `DEFAULT_MAXS_2`, `CROUCH_MAXS_2` (`bg_public.h:74-76`).
pub(crate) const DEFAULT_MINS_2: i32 = -24;
const DEFAULT_MAXS_2: i32 = 40;
const CROUCH_MAXS_2: i32 = 16;
/// `NUM_FORCE_POWERS`.
pub const NPC_FORCE_POWERS: usize = 18;
/// `class_t`'s `CLASS_VEHICLE`.
pub const CLASS_VEHICLE: i32 = 53;
/// `SVF_NO_BASIC_SOUNDS`, `SVF_NO_COMBAT_SOUNDS`, `SVF_NO_EXTRA_SOUNDS` (`g_public.h:61-63`):
/// the spawner's sound flags, handed to the NPC before its parse.
pub const SVF_NO_BASIC_SOUNDS: u32 = 0x1000_0000;
pub const SVF_NO_COMBAT_SOUNDS: u32 = 0x2000_0000;
pub const SVF_NO_EXTRA_SOUNDS: u32 = 0x4000_0000;

/// `NPCParms`: every NPC file, compressed and joined.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NpcParms {
    text: Vec<u8>,
    rigid_models: Vec<crate::npc_rigid::RigidModel>,
}

/// Why the NPC files could not be joined, where the game stops the map (`ERR_DROP`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NpcParmsTooLarge {
    /// The file that did not fit.
    pub file: String,
}

impl std::fmt::Display for NpcParmsTooLarge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "NPC extensions (*.npc) are too large: ran out of space before reading {}",
            self.file
        )
    }
}

impl std::error::Error for NpcParmsTooLarge {}

/// The vehicle an NPC is spawned as (`NPC_Vehicle`, `npc spawn vehicle`): the spawn sets
/// the class to `CLASS_VEHICLE` and gives it the vehicle's definition before the parse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcVehicle {
    /// A `VH_FIGHTER`, whose `height` centres its box on its origin.
    Fighter,
    /// Any other kind.
    Other,
}

/// What the spawn has set on the entity before the parse (`NPC_Spawn_Do`,
/// `NPC_spawn.c:1407-1575`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NpcSpawn {
    /// The vehicle, for an NPC spawned as one.
    pub vehicle: Option<NpcVehicle>,
    /// `s.origin`, which a fighter's `height` moves.
    pub origin: [f32; 3],
    /// The `SVF_NO_*_SOUNDS` flags handed on from the spawner.
    pub sound_flags: u32,
}

/// `gNPCstats_t` (`b_public.h:105-126`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NpcStats {
    pub aggression: i32,
    pub aim: i32,
    pub earshot: f32,
    pub evasion: i32,
    pub hfov: i32,
    pub intelligence: i32,
    /// `move`.
    pub movement: i32,
    pub reactions: i32,
    pub shoot_distance: f32,
    pub vfov: i32,
    pub vigilance: f32,
    pub visrange: f32,
    pub run_speed: i32,
    pub walk_speed: i32,
    pub yaw_speed: f32,
    pub health: i32,
    pub acceleration: i32,
}

impl NpcStats {
    /// The defaults the parse fills in (`NPC_stats.c:1003-1022`) over a cleared `gNPC_t`.
    pub const DEFAULT: Self = Self {
        aggression: 3,
        aim: 3,
        earshot: 1024.0,
        evasion: 3,
        hfov: 90,
        intelligence: 3,
        movement: 3,
        reactions: 3,
        shoot_distance: 0.0,
        vfov: 60,
        vigilance: 0.1,
        visrange: 1024.0,
        run_speed: 300,
        walk_speed: 90,
        yaw_speed: 90.0,
        health: 0,
        acceleration: 15,
    };
}

/// `csSounds_Std`, `csSounds_Combat`, `csSounds_Extra`, `csSounds_Jedi`: the sound sets
/// `NPC_Precache` registers, by the name it registers (`*$` and the set's directory).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NpcSounds {
    pub standard: Option<Vec<u8>>,
    pub combat: Option<Vec<u8>>,
    pub extra: Option<Vec<u8>>,
    pub jedi: Option<Vec<u8>>,
}

/// A uniform scale (`scale`): the percentage the client is sent (`ps.iModelScale`) and
/// the entity's own scale, capped below 1024 per cent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NpcScale {
    pub percent: i32,
    pub scale: f32,
}

/// Everything `NPC_ParseParms` writes for one NPC, over a freshly spawned entity.
///
/// Values the reference writes unconditionally are plain; those only a key writes are
/// `Option`s (`None`: the spawn's own value stands), except bit sets, which are what the
/// keys add.
#[derive(Clone, Debug, PartialEq)]
pub struct NpcDefinition {
    /// The name looked for (`Player` for an empty one).
    pub name: Vec<u8>,
    /// `NPC->NPC->stats`.
    pub stats: NpcStats,
    /// `NPC->NPC->rank` (`rank_t`), `RANK_CIVILIAN` unless set.
    pub rank: i32,
    /// `NPC->NPC->defaultBehavior` (`bState_t`).
    pub default_behavior: i32,
    /// `SCF_ALT_FIRE` in `NPC->NPC->scriptFlags`.
    pub alt_fire: bool,
    /// `client->playerTeam` and `s.teamowner` (`npcteam_t`, -1 for a name not known).
    pub player_team: Option<i32>,
    /// `client->enemyTeam`.
    pub enemy_team: Option<i32>,
    /// `client->NPC_class` after the parse (`class_t`, -1 for a name not known).
    pub client_class: i32,
    /// `s.NPC_class`, which only the `class` key sets.
    pub entity_class: i32,
    /// `renderInfo`'s head and torso clamps: yaw left, yaw right, pitch up, pitch down.
    pub head_ranges: [i32; 4],
    pub torso_ranges: [i32; 4],
    /// `ps.customRGBA`.
    pub custom_rgba: [i32; 4],
    pub scale: Option<NpcScale>,
    /// `r.mins`, `r.maxs`.
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// `NPC->radius`, set by `height`.
    pub radius: Option<f32>,
    /// `ps.standheight`, `ps.crouchheight`.
    pub stand_height: i32,
    pub crouch_height: i32,
    /// `s.origin`, moved by a fighter's `height`.
    pub origin: [f32; 3],
    /// `EF2_FLYING` (`movetype flyswim`).
    pub flying: bool,
    /// `ps.weapon`, the last `weapon` key's.
    pub weapon: Option<i32>,
    /// Bits `stats[STAT_WEAPONS]` gains.
    pub weapons: u32,
    /// The `ps.ammo` indices filled to 100, as bits.
    pub ammo_filled: u32,
    /// `ps.fd.forcePowerLevel` for each power a key named (0 to 5); a level above 0 sets
    /// the power's bit in `forcePowersKnown`, 0 clears it.
    pub force_levels: [Option<i32>; NPC_FORCE_POWERS],
    pub force_power_max: Option<i32>,
    /// `ps.fd.saberAnimLevel` (0 to 5).
    pub saber_style: Option<i32>,
    /// `client->saber`: from a cleared client, the keys' sabers and colours.
    pub sabers: [SaberDefinition; 2],
    /// `s.boltToPlayer`: each hand's colour plus one, the first in bits 0-2.
    pub bolt_to_player: i32,
    /// `s.npcSaber1`, `s.npcSaber2`: the `@name` model configstrings.
    pub saber_models: [Option<Vec<u8>>; 2],
    /// The `playerModel`, `customSkin` (never empty) and the comma-joined `surfOff` and
    /// `surfOn` lists (which the multiplayer parse builds and never uses).
    pub player_model: Vec<u8>,
    /// Native-only single-frame MD3 actor; `player_model` is its full asset path.
    pub rigid_model: bool,
    pub custom_skin: Vec<u8>,
    pub surf_off: Vec<u8>,
    pub surf_on: Vec<u8>,
    /// `NPC->fullName` (`G_NewString`: `\n` made a line break).
    pub full_name: Option<Vec<u8>>,
    /// `s.modelindex`'s configstring if the Ghoul2 model loads: `models/players/<model>/
    /// model.glm*<skin>`, or `$<name>` for a vehicle.
    pub model: Vec<u8>,
    /// `csSounds_*`.
    pub sounds: NpcSounds,
    /// Every `G_ModelIndex` the parse makes, in order, repeats included.
    pub registered_models: Vec<Vec<u8>>,
    /// Where in [`Self::registered_models`] the Ghoul2 setup's (`model`) is: the one
    /// registration made only if the model loads.
    pub setup_model_at: usize,
    /// Every `G_SoundIndex` `NPC_Precache` makes, in order.
    pub registered_sounds: Vec<Vec<u8>>,
    /// The weapons `NPC_Precache` registers (`RegisterItem`), in order.
    pub registered_weapons: Vec<i32>,
}

impl NpcDefinition {
    /// `ps.fd.forcePowersKnown` after the parse, from the spawn's `known`.
    pub fn force_powers_known(&self, known: u32) -> u32 {
        self.force_levels
            .iter()
            .enumerate()
            .fold(known, |known, (power, level)| match level {
                Some(0) => known & !(1 << power),
                Some(_) => known | (1 << power),
                None => known,
            })
    }
}

/// Why the reference's parse answers `qfalse`: the NPC is not spawned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NpcRefusalReason {
    /// `random`, which multiplayer does not build.
    Random,
    /// No block by that name.
    NotFound,
    /// The name not followed by `{`.
    NoBlock,
    /// The files end inside the block.
    UnexpectedEof,
    /// `class CLASS_VEHICLE` on an NPC not spawned as a vehicle.
    VehicleWithoutVehicle,
    /// No `playerModel` (nor a vehicle): an MD3 NPC, which multiplayer refuses.
    Md3Model,
    /// A saber the game would stop the map over.
    Saber(SaberDefinitionError),
}

/// A refused NPC, with the models its parse had registered already.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NpcRefusal {
    pub reason: NpcRefusalReason,
    pub registered_models: Vec<Vec<u8>>,
}

impl NpcParms {
    /// `NPC_LoadParms` over the files' contents (the whole file, as `FS_Open` sizes it),
    /// in the listing's order: each compressed and followed by a line break. A file that
    /// would take the text to `MAX_NPC_DATA_SIZE` stops the map; the size checked is the
    /// file's own before compression, against the compressed text so far
    /// (`NPC_stats.c:3594-3597`).
    pub fn load<'a>(
        files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    ) -> Result<Self, NpcParmsTooLarge> {
        Self::load_within(files, MAX_NPC_DATA_SIZE)
    }

    /// [`Self::load`] without `MAX_NPC_DATA_SIZE` (this server's own, outside the stock
    /// rules): every file joined, however large the whole.
    pub fn load_unbounded<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Self {
        Self::load_within(files, usize::MAX).unwrap_or_default()
    }

    /// Joined NPC text without the stock aggregate-size limit.
    pub fn from_joined(text: Vec<u8>) -> Self {
        Self {
            text,
            rigid_models: Vec::new(),
        }
    }

    fn load_within<'a>(
        files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
        bound: usize,
    ) -> Result<Self, NpcParmsTooLarge> {
        let mut text = Vec::new();
        for (name, contents) in files {
            if text.len().saturating_add(contents.len()) >= bound {
                return Err(NpcParmsTooLarge {
                    file: name.to_owned(),
                });
            }
            text.extend_from_slice(&compress(contents));
            text.push(b'\n');
        }
        Ok(Self {
            text,
            rigid_models: Vec::new(),
        })
    }

    /// Enable verified one-piece MD3 NPC models for the native profile. The stock
    /// parser remains unchanged unless the host explicitly supplies these assets.
    pub fn with_rigid_models(mut self, models: Vec<crate::npc_rigid::RigidModel>) -> Self {
        self.rigid_models = models;
        self
    }

    /// The joined text.
    pub fn text(&self) -> &[u8] {
        &self.text
    }

    /// Whether an NPC called `name` (any case) is defined: [`Self::block`] finds it.
    pub fn defines(&self, name: &[u8]) -> bool {
        self.block(name).is_ok()
    }

    /// The parser at the block of the NPC called `name` (any case), past its `{`: the
    /// search of `NPC_stats.c:1109-1133`, each other name's section skipped.
    pub(crate) fn block(&self, name: &[u8]) -> Result<TextParser<'_>, NpcRefusalReason> {
        let mut parser = TextParser::new(&self.text);
        while parser.is_live() {
            let token = parser.parse_ext(true);
            if token.is_empty() {
                return Err(NpcRefusalReason::NotFound);
            }
            if token.eq_ignore_ascii_case(name) {
                break;
            }
            parser.skip_braced_section(0);
        }
        if !parser.is_live() {
            return Err(NpcRefusalReason::NotFound);
        }
        if !parser.parse_literal(b"{") {
            return Err(NpcRefusalReason::NoBlock);
        }
        Ok(parser)
    }

    /// `NPC_ParseParms(name, NPC)` for an NPC spawned as `spawn` says, its sabers read from
    /// `sabers`.
    pub fn parse(
        &self,
        name: &[u8],
        spawn: &NpcSpawn,
        sabers: &SaberParms,
        host: &mut impl SaberParseHost,
    ) -> Result<NpcDefinition, NpcRefusal> {
        let name = until_nul(name);
        let name = if name.is_empty() {
            &b"Player"[..]
        } else {
            name
        };
        let mut npc = defaults(name, spawn);
        let refuse = |reason, npc: NpcDefinition| NpcRefusal {
            reason,
            registered_models: npc.registered_models,
        };
        if name.eq_ignore_ascii_case(b"random") {
            return Err(refuse(NpcRefusalReason::Random, npc));
        }
        let mut parser = match self.block(name) {
            Ok(parser) => parser,
            Err(reason) => return Err(refuse(reason, npc)),
        };
        let mut reader = crate::npc_parms_keys::Reader {
            npc: &mut npc,
            spawn,
            sabers,
            md3_model: true,
        };
        loop {
            let token = parser.parse_ext(true);
            if token.is_empty() {
                return Err(refuse(NpcRefusalReason::UnexpectedEof, npc));
            }
            if token.eq_ignore_ascii_case(b"}") {
                break;
            }
            match reader.key(token, &mut parser, host) {
                Ok(KeyOutcome::Read) => {}
                Ok(KeyOutcome::Unknown) => parser.skip_rest_of_line(),
                Err(reason) => return Err(refuse(reason, npc)),
            }
        }
        let md3_model = reader.md3_model;
        if let Some(model) = self
            .rigid_models
            .iter()
            .find(|model| model.npc.eq_ignore_ascii_case(name))
        {
            npc.player_model = model.path.clone();
            npc.rigid_model = true;
        } else if md3_model {
            return Err(refuse(NpcRefusalReason::Md3Model, npc));
        }
        if let Err(error) = finish(&mut npc, sabers, host) {
            return Err(refuse(NpcRefusalReason::Saber(error), npc));
        }
        crate::npc_precache::precache(self, &mut npc, spawn);
        Ok(npc)
    }
}

/// The entity as the parse leaves it before the block (`NPC_stats.c:987-1085`): the
/// stats' defaults, the look clamps, the player's box and heights, white.
fn defaults(name: &[u8], spawn: &NpcSpawn) -> NpcDefinition {
    NpcDefinition {
        name: name.to_vec(),
        stats: NpcStats::DEFAULT,
        rank: 0,
        default_behavior: 0,
        alt_fire: false,
        player_team: None,
        enemy_team: None,
        client_class: if spawn.vehicle.is_some() {
            CLASS_VEHICLE
        } else {
            0
        },
        entity_class: 0,
        head_ranges: [80, 80, 45, 45],
        torso_ranges: [60, 60, 30, 50],
        custom_rgba: [255; 4],
        scale: None,
        mins: [-15.0, -15.0, DEFAULT_MINS_2 as f32],
        maxs: [15.0, 15.0, DEFAULT_MAXS_2 as f32],
        radius: None,
        stand_height: DEFAULT_MAXS_2,
        crouch_height: CROUCH_MAXS_2,
        origin: spawn.origin,
        flying: false,
        weapon: None,
        weapons: 0,
        ammo_filled: 0,
        force_levels: [None; NPC_FORCE_POWERS],
        force_power_max: None,
        saber_style: None,
        sabers: [SaberDefinition::empty(), SaberDefinition::empty()],
        bolt_to_player: 0,
        saber_models: [None, None],
        player_model: Vec::new(),
        rigid_model: false,
        custom_skin: b"default".to_vec(),
        surf_off: Vec::new(),
        surf_on: Vec::new(),
        full_name: None,
        model: Vec::new(),
        sounds: NpcSounds::default(),
        registered_models: Vec::new(),
        setup_model_at: 0,
        registered_sounds: Vec::new(),
        registered_weapons: Vec::new(),
    }
}

/// The end of the parse (`NPC_stats.c:3517-3549`): no `saber` key means Kyle's saber
/// (over any colours the keys gave the first hand), an empty skin is `default`, a
/// vehicle's model is its name, and the Ghoul2 setup registers the model.
fn finish(
    npc: &mut NpcDefinition,
    sabers: &SaberParms,
    host: &mut impl SaberParseHost,
) -> Result<(), SaberDefinitionError> {
    if npc.saber_models[0].is_none() {
        let model = [b"@".as_slice(), DEFAULT_SABER].concat();
        npc.registered_models.push(model.clone());
        npc.saber_models[0] = Some(model);
        npc.sabers[0] = sabers.parse(DEFAULT_SABER, host)?.1;
    }
    if npc.custom_skin.is_empty() {
        npc.custom_skin = b"default".to_vec();
    }
    npc.model = if npc.rigid_model {
        npc.player_model.clone()
    } else if npc.client_class == CLASS_VEHICLE {
        // `va("$%s", NPCName)`, and the setup keeps `MAX_QPATH` of it.
        crate::saber_definition::truncated(&[b"$".as_slice(), &npc.name].concat(), MAX_QPATH)
    } else {
        [
            b"models/players/".as_slice(),
            &npc.player_model,
            b"/model.glm*",
            &npc.custom_skin,
        ]
        .concat()
    };
    npc.setup_model_at = npc.registered_models.len();
    npc.registered_models.push(npc.model.clone());
    Ok(())
}

/// `G_NewString`: `\n` made a line break; every other byte kept.
pub(crate) fn new_string(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        if text[index] == b'\\' && text.get(index + 1) == Some(&b'n') {
            out.push(b'\n');
            index += 2;
        } else {
            out.push(text[index]);
            index += 1;
        }
    }
    out
}
