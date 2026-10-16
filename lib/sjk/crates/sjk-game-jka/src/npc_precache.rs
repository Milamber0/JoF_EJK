//! `NPC_Precache` (`NPC_stats.c:589-859`) as `NPC_ParseParms` runs it on the NPC it has
//! just read: the block read again, by rules of its own, for what to register — the sound
//! sets (`csSounds_*`), the weapons named, the model, and the weapons its team would carry
//! (`NPC_PrecacheWeapons`, `NPC_WeaponsForTeam` in `NPC_spawn.c:525-748`).
//!
//! Its reading differs from the parse's: only a few keys are known, nothing else skips its
//! line (every word of an unknown key's line is read as a key), `headmodel`, `torsomodel`
//! and `legsmodel` make it an MD3 model again until the next `playerModel`, and an empty
//! `customSkin` stays empty (the model is registered without a skin).

use crate::npc_parms::{
    CLASS_VEHICLE, MAX_QPATH, NpcDefinition, NpcParms, NpcSpawn, SVF_NO_BASIC_SOUNDS,
    SVF_NO_COMBAT_SOUNDS, SVF_NO_EXTRA_SOUNDS,
};
use crate::saber_definition::truncated;

/// `weapon_t` numbers `NPC_WeaponsForTeam` hands out.
const WP_STUN_BATON: i32 = 1;
const WP_SABER: i32 = 3;
const WP_BLASTER: i32 = 5;
const WP_DISRUPTOR: i32 = 6;
const WP_BOWCASTER: i32 = 7;
const WP_REPEATER: i32 = 8;
const WP_FLECHETTE: i32 = 10;
const WP_ROCKET_LAUNCHER: i32 = 11;
const WP_THERMAL: i32 = 12;
/// `WP_NUM_WEAPONS`.
const WP_NUM_WEAPONS: i32 = 19;
/// `npcteam_t`'s `NPCTEAM_ENEMY`, `NPCTEAM_PLAYER`.
const NPCTEAM_ENEMY: i32 = 1;
const NPCTEAM_PLAYER: i32 = 2;

/// Which sound set a key names, and the spawner flag that silences it.
fn sound_slot(key: &[u8]) -> Option<u32> {
    Some(match key {
        b"snd" => SVF_NO_BASIC_SOUNDS,
        b"sndcombat" => SVF_NO_COMBAT_SOUNDS,
        b"sndextra" | b"sndjedi" => SVF_NO_EXTRA_SOUNDS,
        _ => return None,
    })
}

/// What `NPC_Precache` registers for one spawner: the models, the sound sets (each
/// kept as the spawner's `csSounds_*` too) and the weapons, in the order it registers
/// them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Precached {
    /// `G_ModelIndex`'s names.
    pub models: Vec<Vec<u8>>,
    /// `G_SoundIndex`'s names.
    pub sounds: Vec<Vec<u8>>,
    /// The weapons `RegisterItem` registers.
    pub weapons: Vec<i32>,
    /// `s.csSounds_*`: the sound set each key named, by the name registered.
    pub sound_sets: crate::npc_parms::NpcSounds,
}

/// `NPC_Precache(spawner)` (`NPC_stats.c:589-859`) for a map spawner as `SP_NPC_spawner`
/// runs it: the NPC called `name` (as the map wrote it; `random` and no name precache
/// nothing) looked up and read by the precache's own rules, the spawner's sound flags
/// silencing its sound sets and its spawn flags choosing its team's weapons. A spawner
/// has no client, so it is never a vehicle here.
pub fn spawner_precache(
    parms: &NpcParms,
    name: Option<&[u8]>,
    sound_flags: u32,
    spawnflags: i32,
) -> Precached {
    let mut out = Precached::default();
    let Some(name) = name.map(crate::text_parse::until_nul) else {
        return out;
    };
    if name.eq_ignore_ascii_case(b"random") {
        return out;
    }
    read_block(parms, name, false, sound_flags, spawnflags, &mut out);
    out
}

/// `NPC_Precache(NPC)`: the registrations it makes, added to `npc`. The entity being
/// parsed has no spawn flags yet.
pub(crate) fn precache(parms: &NpcParms, npc: &mut NpcDefinition, spawn: &NpcSpawn) {
    let mut out = Precached::default();
    read_block(
        parms,
        &npc.name.clone(),
        npc.client_class == CLASS_VEHICLE,
        spawn.sound_flags,
        0,
        &mut out,
    );
    npc.registered_models.append(&mut out.models);
    npc.registered_sounds.append(&mut out.sounds);
    npc.registered_weapons.append(&mut out.weapons);
    let sets = std::mem::take(&mut out.sound_sets);
    for (slot, set) in [
        (&mut npc.sounds.standard, sets.standard),
        (&mut npc.sounds.combat, sets.combat),
        (&mut npc.sounds.extra, sets.extra),
        (&mut npc.sounds.jedi, sets.jedi),
    ] {
        if set.is_some() {
            *slot = set;
        }
    }
}

/// The block of `name` read by the precache's rules into `out`.
fn read_block(
    parms: &NpcParms,
    name: &[u8],
    vehicle: bool,
    sound_flags: u32,
    spawnflags: i32,
    out: &mut Precached,
) {
    let Ok(mut parser) = parms.block(name) else {
        return;
    };
    let (mut md3_model, mut player_model, mut custom_skin, mut team) =
        (false, Vec::new(), b"default".to_vec(), 0);
    loop {
        let token = parser.parse_ext(true);
        if token.is_empty() {
            // "unexpected EOF": nothing more is registered.
            return;
        }
        let key = token.to_ascii_lowercase();
        match key.as_slice() {
            b"}" => break,
            b"headmodel" | b"torsomodel" | b"legsmodel" => {
                parser.parse_string();
                md3_model = true;
            }
            b"playermodel" => {
                player_model = truncated(parser.parse_string(), MAX_QPATH);
                md3_model = false;
            }
            b"customskin" => custom_skin = truncated(parser.parse_string(), MAX_QPATH),
            b"playerteam" => {
                let value = parser.parse_string();
                team = crate::npc_parms_keys::team(value);
            }
            b"weapon" => {
                let weapon =
                    crate::weapon_data::weapon_by_name(parser.parse_string()).unwrap_or(-1);
                if weapon > 0 && weapon < WP_NUM_WEAPONS {
                    out.weapons.push(weapon);
                }
            }
            _ => {
                let Some(flag) = sound_slot(&key) else {
                    continue;
                };
                let value = parser.parse_string();
                if sound_flags & flag == 0 {
                    register_sound(out, &key, value);
                }
            }
        }
    }
    // A vehicle's model is its name, registered by the Ghoul2 setup.
    if !vehicle && !md3_model {
        // `Com_sprintf` into `MAX_QPATH`, then the skin appended past it.
        let mut model = truncated(
            &[b"models/players/".as_slice(), &player_model, b"/model.glm"].concat(),
            MAX_QPATH,
        );
        if !custom_skin.is_empty() {
            model.push(b'*');
            model.extend_from_slice(&custom_skin);
        }
        out.models.push(model);
    }
    let weapons = weapons_for_team(team, spawnflags, name);
    // `NPC_PrecacheWeapons` walks from the saber: a stun baton is never registered.
    out.weapons
        .extend((WP_SABER..WP_NUM_WEAPONS).filter(|weapon| weapons & (1 << weapon) != 0));
}

/// A sound set's name registered (`*$` and the value up to its first `/`) and kept as the
/// entity's `csSounds_*`.
fn register_sound(out: &mut Precached, key: &[u8], value: &[u8]) {
    let mut set = truncated(value, MAX_QPATH);
    if let Some(slash) = set.iter().position(|&byte| byte == b'/') {
        set.truncate(slash);
    }
    let name = [b"*$".as_slice(), &set].concat();
    out.sounds.push(name.clone());
    let slot = match key {
        b"snd" => &mut out.sound_sets.standard,
        b"sndcombat" => &mut out.sound_sets.combat,
        b"sndextra" => &mut out.sound_sets.extra,
        _ => &mut out.sound_sets.jedi,
    };
    *slot = Some(name);
}

/// `SFB_RIFLEMAN`, `SFB_PHASER` (`b_local.h:155-156`): a player-team NPC's spawn flags
/// that arm it with a repeater or a blaster.
const SFB_RIFLEMAN: i32 = 2;
const SFB_PHASER: i32 = 4;

/// `NPC_WeaponsForTeam(team, spawnflags, NPC_type)` (`NPC_spawn.c:525-748`): the weapons
/// an NPC of that team and name carries by default, as bits.
pub fn weapons_for_team(team: i32, spawnflags: i32, name: &[u8]) -> u32 {
    // `Q_stricmp` against a name, `Q_strncmp` (case counts) against a prefix.
    let is = |known: &str| name.eq_ignore_ascii_case(known.as_bytes());
    let starts = |prefix: &str| name.starts_with(prefix.as_bytes());
    let bit = |weapon: i32| 1u32 << weapon;
    match team {
        NPCTEAM_ENEMY => {
            if is("tavion") || starts("reborn") || is("desann") || starts("shadowtrooper") {
                return bit(WP_SABER);
            }
            const ENEMIES: [(&str, bool, u32); 30] = [
                ("stofficer", true, 1 << WP_FLECHETTE),
                ("stcommander", false, 1 << WP_REPEATER),
                ("swamptrooper", false, 1 << WP_FLECHETTE),
                ("swamptrooper2", false, 1 << WP_REPEATER),
                ("rockettrooper", false, 1 << WP_ROCKET_LAUNCHER),
                ("shadowtrooper", true, 1 << WP_SABER),
                ("imperial", false, 1 << WP_BLASTER),
                ("impworker", true, 1 << WP_BLASTER),
                ("stormpilot", false, 1 << WP_BLASTER),
                ("galak", false, 1 << WP_BLASTER),
                ("galak_mech", false, 1 << WP_REPEATER),
                ("ugnaught", true, 0),
                ("granshooter", false, 1 << WP_BLASTER),
                ("granboxer", false, 1 << WP_STUN_BATON),
                ("gran", true, (1 << WP_THERMAL) | (1 << WP_STUN_BATON)),
                ("rodian", false, 1 << WP_DISRUPTOR),
                ("rodian2", false, 1 << WP_BLASTER),
                ("interrogator", false, 0),
                ("sentry", false, 0),
                ("protocol", true, 0),
                ("weequay", true, 1 << WP_BOWCASTER),
                ("impofficer", false, 1 << WP_BLASTER),
                ("impcommander", false, 1 << WP_BLASTER),
                ("probe", false, 0),
                ("seeker", false, 0),
                ("remote", false, 0),
                ("trandoshan", false, 1 << WP_REPEATER),
                ("atst", false, 0),
                ("mark1", false, 0),
                ("mark2", false, 0),
            ];
            const MONSTERS: [&str; 2] = ["minemonster", "howler"];
            if let Some((_, _, weapons)) = ENEMIES
                .iter()
                .find(|(known, prefix, _)| if *prefix { starts(known) } else { is(known) })
            {
                return *weapons;
            }
            if MONSTERS.iter().any(|known| is(known)) {
                return bit(WP_STUN_BATON);
            }
            // "Stormtroopers, etc."
            bit(WP_BLASTER)
        }
        NPCTEAM_PLAYER => {
            if spawnflags & SFB_RIFLEMAN != 0 {
                bit(WP_REPEATER)
            } else if spawnflags & SFB_PHASER != 0 {
                bit(WP_BLASTER)
            } else if starts("jedi") || is("luke") {
                bit(WP_SABER)
            } else if starts("prisoner") || is("MonMothma") {
                0
            } else {
                // `bespincop` and the rebels alike.
                bit(WP_BLASTER)
            }
        }
        // NPCTEAM_NEUTRAL names only NPCs with no weapon; every other team has none.
        _ => 0,
    }
}
