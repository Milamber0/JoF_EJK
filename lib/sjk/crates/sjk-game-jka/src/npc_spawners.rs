//! The map's NPC spawners (`codemp/game/NPC_spawn.c:1822-3950`): the `NPC_*` classnames of
//! the spawn table (`g_spawn.c:563-634`), each of which picks a type — some by their spawn
//! flags, some by `Q_irand` — and registers what its class needs, then `SP_NPC_spawner`,
//! which reads the spawner's keys and precaches the NPC.
//!
//! [`choose`] is the classname's own function up to `SP_NPC_spawner`: one table for all of
//! them. [`NpcSpawner::from_entity`] is `G_SpawnGEntityFromSpawnVars`' field parse
//! (`G_ParseField`) and `SP_NPC_spawner` itself. What the precache registers is
//! [`crate::npc_precache::spawner_precache`]'s.
//!
//! `NPC_Vehicle` (`SP_NPC_Vehicle`) is recognised and marked; vehicles are the NPC plan's
//! step 10 (matrix row A03), and whoever spawns them says so.
//!
//! Held to `tools/game-oracle/npcspawn.c` (`game-npcspawn.txt`).

use crate::items::{ITEMS, Kind};
use crate::text_parse::atof;
use crate::userinfo::atoi;

/// `SP_NPC_spawner` without a `wait` key: half a second between tries.
const DEFAULT_WAIT: f32 = 500.0;
/// `START_TIME_REMOVE_ENTS + 50` (`g_local.h:54-65`): a spawner that nobody uses spawns
/// this long after the map starts.
pub const AUTO_SPAWN_DELAY: i32 = 100 * 3 + 50;
/// `SVF_NO_BASIC_SOUNDS` and the two others, by the key that sets each.
const SOUND_KEYS: [(&str, u32); 3] = [
    ("noBasicSounds", crate::npc_parms::SVF_NO_BASIC_SOUNDS),
    ("noCombatSounds", crate::npc_parms::SVF_NO_COMBAT_SOUNDS),
    ("noExtraSounds", crate::npc_parms::SVF_NO_EXTRA_SOUNDS),
];

/// Something a class's own precache (`NPC_*_Precache`) registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Registration {
    /// `G_SoundIndex`.
    Sound(&'static str),
    /// `G_EffectIndex`.
    Effect(&'static str),
    /// `RegisterItem(BG_FindItemForWeapon(weapon))`.
    Weapon(i32),
    /// `RegisterItem(BG_FindItemForAmmo(ammo))`.
    Ammo(i32),
}

impl Registration {
    /// The item list's row a weapon or ammo registration names (`BG_FindItemForWeapon`,
    /// `BG_FindItemForAmmo`: the first of that kind and tag), `None` for the others.
    pub fn item(self) -> Option<usize> {
        let (kind, tag) = match self {
            Self::Weapon(tag) => (Kind::Weapon, tag),
            Self::Ammo(tag) => (Kind::Ammo, tag),
            Self::Sound(_) | Self::Effect(_) => return None,
        };
        ITEMS
            .iter()
            .position(|item| item.kind == kind && item.tag == tag)
    }
}

use Registration::{Ammo, Effect, Sound, Weapon};

/// `AMMO_FORCE`, `AMMO_BLASTER`, `AMMO_POWERCELL`, `AMMO_METAL_BOLTS`; `WP_BOWCASTER`,
/// `WP_ROCKET_LAUNCHER`, `WP_BRYAR_PISTOL`.
const AMMO_FORCE: i32 = 1;
const AMMO_BLASTER: i32 = 2;
const AMMO_POWERCELL: i32 = 3;
const AMMO_METAL_BOLTS: i32 = 4;
const WP_BOWCASTER: i32 = 7;
const WP_ROCKET_LAUNCHER: i32 = 11;
const WP_BRYAR_PISTOL: i32 = 16;

/// The classes' precaches (`NPC_AI_*.c`), each as its function registers.
const SHADOWTROOPER: &[Registration] = &[
    Ammo(AMMO_FORCE),
    Sound("sound/chars/shadowtrooper/cloak.wav"),
    Sound("sound/chars/shadowtrooper/decloak.wav"),
];
const GONK: &[Registration] = &[
    Sound("sound/chars/gonk/misc/gonktalk1.wav"),
    Sound("sound/chars/gonk/misc/gonktalk2.wav"),
    Sound("sound/chars/gonk/misc/death1.wav"),
    Sound("sound/chars/gonk/misc/death2.wav"),
    Sound("sound/chars/gonk/misc/death3.wav"),
    Effect("env/med_explode"),
];
const MOUSE: &[Registration] = &[
    Sound("sound/chars/mouse/misc/mousego1.wav"),
    Sound("sound/chars/mouse/misc/mousego2.wav"),
    Sound("sound/chars/mouse/misc/mousego3.wav"),
    Effect("env/small_explode"),
    Sound("sound/chars/mouse/misc/death1"),
    Sound("sound/chars/mouse/misc/mouse_lp"),
];
const SEEKER: &[Registration] = &[
    Sound("sound/chars/seeker/misc/fire.wav"),
    Sound("sound/chars/seeker/misc/hiss.wav"),
    Effect("env/small_explode"),
];
const REMOTE: &[Registration] = &[
    Sound("sound/chars/remote/misc/fire.wav"),
    Sound("sound/chars/remote/misc/hiss.wav"),
    Effect("env/small_explode"),
];
const R2D2: &[Registration] = &[
    Sound("sound/chars/r2d2/misc/r2d2talk01.wav"),
    Sound("sound/chars/r2d2/misc/r2d2talk02.wav"),
    Sound("sound/chars/r2d2/misc/r2d2talk03.wav"),
    Sound("sound/chars/mark2/misc/mark2_explo"),
    Sound("sound/chars/r2d2/misc/r2_move_lp.wav"),
    Effect("env/med_explode"),
    Effect("volumetric/droid_smoke"),
    Effect("sparks/spark"),
    Effect("chunks/r2d2head"),
    Effect("chunks/r2d2head_veh"),
];
const R5D2: &[Registration] = &[
    Sound("sound/chars/r5d2/misc/r5talk1.wav"),
    Sound("sound/chars/r5d2/misc/r5talk2.wav"),
    Sound("sound/chars/r5d2/misc/r5talk3.wav"),
    Sound("sound/chars/r5d2/misc/r5talk4.wav"),
    Sound("sound/chars/mark2/misc/mark2_explo"),
    Sound("sound/chars/r2d2/misc/r2_move_lp2.wav"),
    Effect("env/med_explode"),
    Effect("volumetric/droid_smoke"),
    Effect("sparks/spark"),
    Effect("chunks/r5d2head"),
    Effect("chunks/r5d2head_veh"),
];
const PROBE: &[Registration] = &[
    Sound("sound/chars/probe/misc/probetalk1"),
    Sound("sound/chars/probe/misc/probetalk2"),
    Sound("sound/chars/probe/misc/probetalk3"),
    Sound("sound/chars/probe/misc/probedroidloop"),
    Sound("sound/chars/probe/misc/anger1"),
    Sound("sound/chars/probe/misc/fire"),
    Effect("chunks/probehead"),
    Effect("env/med_explode2"),
    Effect("explosions/probeexplosion1"),
    Effect("bryar/muzzle_flash"),
    Ammo(AMMO_BLASTER),
    Weapon(WP_BRYAR_PISTOL),
];
const INTERROGATOR: &[Registration] = &[
    Sound("sound/chars/interrogator/misc/torture_droid_lp"),
    Sound("sound/chars/mark1/misc/anger.wav"),
    Sound("sound/chars/probe/misc/talk"),
    Sound("sound/chars/interrogator/misc/torture_droid_inject"),
    Sound("sound/chars/interrogator/misc/int_droid_explo"),
    Effect("explosions/droidexplosion1"),
];
const MINEMONSTER: &[Registration] = &[
    Sound("sound/chars/mine/misc/bite1.wav"),
    Sound("sound/chars/mine/misc/miss1.wav"),
    Sound("sound/chars/mine/misc/bite2.wav"),
    Sound("sound/chars/mine/misc/miss2.wav"),
    Sound("sound/chars/mine/misc/bite3.wav"),
    Sound("sound/chars/mine/misc/miss3.wav"),
    Sound("sound/chars/mine/misc/bite4.wav"),
    Sound("sound/chars/mine/misc/miss4.wav"),
];
const ATST: &[Registration] = &[
    Sound("sound/chars/atst/atst_damaged1"),
    Sound("sound/chars/atst/atst_damaged2"),
    Weapon(WP_BOWCASTER),
    Weapon(WP_ROCKET_LAUNCHER),
    Effect("env/med_explode2"),
    Effect("blaster/smoke_bolton"),
    Effect("explosions/droidexplosion1"),
];
const SENTRY: &[Registration] = &[
    Sound("sound/chars/sentry/misc/sentry_explo"),
    Sound("sound/chars/sentry/misc/sentry_pain"),
    Sound("sound/chars/sentry/misc/sentry_shield_open"),
    Sound("sound/chars/sentry/misc/sentry_shield_close"),
    Sound("sound/chars/sentry/misc/sentry_hover_1_lp"),
    Sound("sound/chars/sentry/misc/sentry_hover_2_lp"),
    Sound("sound/chars/sentry/misc/talk1"),
    Sound("sound/chars/sentry/misc/talk2"),
    Sound("sound/chars/sentry/misc/talk3"),
    Effect("bryar/muzzle_flash"),
    Effect("env/med_explode"),
    Ammo(AMMO_BLASTER),
];
const MARK1: &[Registration] = &[
    Sound("sound/chars/mark1/misc/mark1_wakeup"),
    Sound("sound/chars/mark1/misc/shutdown"),
    Sound("sound/chars/mark1/misc/walk"),
    Sound("sound/chars/mark1/misc/run"),
    Sound("sound/chars/mark1/misc/death1"),
    Sound("sound/chars/mark1/misc/death2"),
    Sound("sound/chars/mark1/misc/anger"),
    Sound("sound/chars/mark1/misc/mark1_fire"),
    Sound("sound/chars/mark1/misc/mark1_pain"),
    Sound("sound/chars/mark1/misc/mark1_explo"),
    Effect("env/med_explode2"),
    Effect("explosions/probeexplosion1"),
    Effect("blaster/smoke_bolton"),
    Effect("bryar/muzzle_flash"),
    Effect("explosions/droidexplosion1"),
    Ammo(AMMO_METAL_BOLTS),
    Ammo(AMMO_BLASTER),
    Weapon(WP_BOWCASTER),
    Weapon(WP_BRYAR_PISTOL),
];
const MARK2: &[Registration] = &[
    Sound("sound/chars/mark2/misc/mark2_explo"),
    Sound("sound/chars/mark2/misc/mark2_pain"),
    Sound("sound/chars/mark2/misc/mark2_fire"),
    Sound("sound/chars/mark2/misc/mark2_move_lp"),
    Effect("explosions/droidexplosion1"),
    Effect("env/med_explode2"),
    Effect("blaster/smoke_bolton"),
    Effect("bryar/muzzle_flash"),
    Weapon(WP_BRYAR_PISTOL),
    Ammo(AMMO_METAL_BOLTS),
    Ammo(AMMO_POWERCELL),
    Ammo(AMMO_BLASTER),
];
const GALAKMECH: &[Registration] = &[
    Sound("sound/weapons/galak/skewerhit.wav"),
    Sound("sound/weapons/galak/lasercharge.wav"),
    Sound("sound/weapons/galak/lasercutting.wav"),
    Sound("sound/weapons/galak/laserdamage.wav"),
    Effect("galak/trace_beam"),
    Effect("galak/beam_warmup"),
    Effect("env/med_explode2"),
    Effect("env/small_explode2"),
    Effect("galak/explode"),
    Effect("blaster/smoke_bolton"),
];
const PROTOCOL: &[Registration] = &[
    Sound("sound/chars/mark2/misc/mark2_explo"),
    Effect("env/med_explode"),
];
const WAMPA: &[Registration] = &[Sound("sound/chars/rancor/swipehit.wav")];
/// `Boba_Precache` (`NPC_AI_Jedi.c:202-209`), which `NPC_Begin` runs for Boba Fett.
pub const BOBA: &[Registration] = &[
    Sound("sound/boba/jeton.wav"),
    Sound("sound/boba/jethover.wav"),
    Sound("sound/effects/combustfire.mp3"),
    Effect("boba/jet"),
    Effect("boba/fthrw"),
];

/// What a classname's spawn function decided before `SP_NPC_spawner`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassChoice {
    /// `NPC_type` as it now stands.
    pub npc_type: Option<Vec<u8>>,
    /// The spawn flags as it left them (`NPC_StormtrooperOfficer` sets one, a random
    /// cultist draws them).
    pub spawnflags: i32,
    /// The class's precache before `SP_NPC_spawner` and after it.
    pub before: &'static [Registration],
    pub after: &'static [Registration],
    /// `NPC_Vehicle`: `SP_NPC_Vehicle` instead of `SP_NPC_spawner`.
    pub vehicle: bool,
}

/// A type set whatever the map said, or only where it said none.
fn set(npc_type: &mut Option<Vec<u8>>, name: &str) {
    *npc_type = Some(name.as_bytes().to_vec());
}

/// `SP_NPC_*` for `classname` (any case, as `spawncmp` compares) up to its call of
/// `SP_NPC_spawner`: the type it picks, in the order of its tests and draws
/// (`NPC_spawn.c:2320-3950`), and the class precache around the spawner. `None` for a
/// classname that is no NPC spawner.
pub fn choose(
    classname: &str,
    npc_type: Option<&[u8]>,
    spawnflags: i32,
    irand: &mut dyn FnMut(i32, i32) -> i32,
) -> Option<ClassChoice> {
    let name = classname.to_ascii_lowercase();
    let mut choice = ClassChoice {
        npc_type: npc_type.map(<[u8]>::to_vec),
        spawnflags,
        before: &[],
        after: &[],
        vehicle: false,
    };
    let flags = spawnflags;
    let unset = choice.npc_type.is_none();
    let t = &mut choice.npc_type;
    let pick = |t: &mut Option<Vec<u8>>, rules: &[(i32, &str)], otherwise: &str| {
        let chosen = rules
            .iter()
            .find(|(bit, _)| flags & bit != 0)
            .map_or(otherwise, |(_, name)| name);
        set(t, chosen);
    };
    match name.as_str() {
        "npc_spawner" => {}
        "npc_vehicle" => {
            // `SP_NPC_Vehicle`: a swoop unless the map named one.
            if unset {
                set(t, "swoop");
            }
            choice.vehicle = true;
        }
        "npc_kyle" => set(t, "Kyle"),
        "npc_lando" => set(t, "Lando"),
        "npc_jan" => set(t, "Jan"),
        "npc_luke" => set(t, "Luke"),
        "npc_monmothma" => set(t, "MonMothma"),
        "npc_tavion" => set(t, "Tavion"),
        "npc_tavion_new" => pick(
            t,
            &[(1, "tavion_scepter"), (2, "tavion_sith_sword")],
            "tavion_new",
        ),
        "npc_alora" => pick(t, &[(1, "alora_dual")], "alora"),
        "npc_reborn_new" if unset => {
            if flags & 4 != 0 {
                pick(
                    t,
                    &[(1, "reborn_dual2"), (2, "reborn_staff2")],
                    "reborn_new2",
                );
            } else {
                pick(t, &[(1, "reborn_dual"), (2, "reborn_staff")], "reborn_new");
            }
        }
        "npc_cultist_saber" if unset => cultist_saber(t, flags, false),
        "npc_cultist_saber_powers" if unset => cultist_saber(t, flags, true),
        "npc_cultist" if unset => {
            if flags & 1 != 0 {
                // A random saber cultist: the flags drawn, then `SP_NPC_Cultist_Saber`.
                let mut drawn = [1, 2, 4][irand(0, 2) as usize];
                if irand(0, 1) != 0 {
                    drawn |= 8;
                }
                choice.spawnflags = drawn;
                cultist_saber(&mut choice.npc_type, drawn, false);
            } else {
                pick(
                    t,
                    &[
                        (2, "cultist_grip"),
                        (4, "cultist_lightning"),
                        (8, "cultist_drain"),
                    ],
                    "cultist",
                );
            }
        }
        "npc_cultist_commando" if unset => set(t, "cultistcommando"),
        "npc_cultist_destroyer" => set(t, "cultist"),
        "npc_reelo" => set(t, "Reelo"),
        "npc_galak" => {
            if flags & 1 != 0 {
                set(t, "Galak_Mech");
                choice.before = GALAKMECH;
            } else {
                set(t, "Galak");
            }
        }
        "npc_desann" | "npc_manuel_vergara_rmg" => set(t, "Desann"),
        "npc_bartender" => set(t, "Bartender"),
        "npc_morgankatarn" => set(t, "MorganKatarn"),
        "npc_jedi" if unset => {
            if flags & 4 != 0 {
                const JEDI: [&str; 12] = [
                    "jedi_hf1",
                    "jedi_hf2",
                    "jedi_hm1",
                    "jedi_hm2",
                    "jedi_kdm1",
                    "jedi_kdm2",
                    "jedi_rm1",
                    "jedi_rm2",
                    "jedi_tf1",
                    "jedi_tf2",
                    "jedi_zf1",
                    "jedi_zf2",
                ];
                set(t, JEDI[irand(0, 11).clamp(0, 11) as usize]);
            } else if flags & 2 != 0 {
                set(t, "jedimaster");
            } else if flags & 1 != 0 {
                set(t, "jeditrainer");
            } else {
                set(t, if irand(0, 1) != 0 { "Jedi" } else { "Jedi2" });
            }
        }
        "npc_prisoner" if unset => set(
            t,
            if irand(0, 1) != 0 {
                "Prisoner"
            } else {
                "Prisoner2"
            },
        ),
        "npc_rebel" if unset => set(t, if irand(0, 1) != 0 { "Rebel" } else { "Rebel2" }),
        "npc_human_merc" if unset => {
            pick(
                t,
                &[
                    (1, "human_merc_bow"),
                    (2, "human_merc_rep"),
                    (4, "human_merc_flc"),
                    (8, "human_merc_cnc"),
                ],
                "human_merc",
            );
        }
        "npc_stormtrooper" | "npc_stormtrooperofficer" => {
            if name == "npc_stormtrooperofficer" {
                choice.spawnflags |= 1;
            }
            let flags = choice.spawnflags;
            let t = &mut choice.npc_type;
            match [8, 4, 2, 1].into_iter().find(|bit| flags & bit != 0) {
                Some(8) => set(t, "rockettrooper"),
                Some(4) => set(t, "stofficeralt"),
                Some(2) => set(t, "stcommander"),
                Some(_) => set(t, "stofficer"),
                None => set(
                    t,
                    if irand(0, 1) != 0 {
                        "StormTrooper"
                    } else {
                        "StormTrooper2"
                    },
                ),
            }
        }
        "npc_snowtrooper" => set(t, "snowtrooper"),
        "npc_tie_pilot" => set(t, "stormpilot"),
        "npc_ugnaught" if unset => set(
            t,
            if irand(0, 1) != 0 {
                "Ugnaught"
            } else {
                "Ugnaught2"
            },
        ),
        "npc_jawa" if unset => pick(t, &[(1, "jawa_armed")], "jawa"),
        "npc_gran" if unset => {
            let chosen = if flags & 1 != 0 {
                "granshooter"
            } else if flags & 2 != 0 {
                "granboxer"
            } else if irand(0, 1) != 0 {
                "gran"
            } else {
                "gran2"
            };
            set(t, chosen);
        }
        "npc_rodian" if unset => pick(t, &[(1, "rodian2")], "rodian"),
        "npc_weequay" if unset => set(
            t,
            ["Weequay", "Weequay2", "Weequay3", "Weequay4"][irand(0, 3).clamp(0, 3) as usize],
        ),
        "npc_trandoshan" if unset => set(t, "Trandoshan"),
        "npc_tusken" if unset => pick(t, &[(1, "tuskensniper")], "tusken"),
        "npc_noghri" if unset => set(t, "noghri"),
        "npc_swamptrooper" if unset => pick(t, &[(1, "SwampTrooper2")], "SwampTrooper"),
        "npc_imperial" if unset => pick(t, &[(1, "ImpOfficer"), (2, "ImpCommander")], "Imperial"),
        "npc_impworker" if unset => {
            let chosen = if irand(0, 2) == 0 {
                "ImpWorker"
            } else if irand(0, 1) != 0 {
                "ImpWorker2"
            } else {
                "ImpWorker3"
            };
            set(t, chosen);
        }
        "npc_bespincop" if unset => set(
            t,
            if irand(0, 1) == 0 {
                "BespinCop"
            } else {
                "BespinCop2"
            },
        ),
        "npc_reborn" | "npc_colombian_rebel" | "npc_colombian_soldier" if unset => {
            pick(
                t,
                &[
                    (1, "rebornforceuser"),
                    (2, "rebornfencer"),
                    (4, "rebornacrobat"),
                    (8, "rebornboss"),
                ],
                "reborn",
            );
        }
        "npc_shadowtrooper" | "npc_colombian_emplacedgunner" => {
            if unset {
                set(
                    t,
                    if irand(0, 1) == 0 {
                        "ShadowTrooper"
                    } else {
                        "ShadowTrooper2"
                    },
                );
            }
            choice.before = SHADOWTROOPER;
        }
        "npc_monster_murjj" => set(t, "Murjj"),
        "npc_monster_swamp" => set(t, "Swamp"),
        "npc_monster_howler" => set(t, "howler"),
        "npc_minemonster" => {
            set(t, "minemonster");
            choice.after = MINEMONSTER;
        }
        "npc_monster_claw" => set(t, "Claw"),
        "npc_monster_glider" => set(t, "Glider"),
        "npc_monster_flier2" => set(t, "Flier2"),
        "npc_monster_lizard" => set(t, "Lizard"),
        "npc_monster_fish" => set(t, "Fish"),
        "npc_monster_wampa" => {
            set(t, "wampa");
            choice.before = WAMPA;
        }
        "npc_monster_rancor" => set(t, "rancor"),
        "npc_droid_interrogator" => droid(&mut choice, "interrogator", INTERROGATOR),
        "npc_droid_probe" => droid(&mut choice, "probe", PROBE),
        "npc_droid_mark1" => droid(&mut choice, "mark1", MARK1),
        "npc_droid_mark2" => droid(&mut choice, "mark2", MARK2),
        "npc_droid_atst" => droid(
            &mut choice,
            if flags & 1 != 0 {
                "atst_vehicle"
            } else {
                "atst"
            },
            ATST,
        ),
        "npc_droid_remote" => droid(&mut choice, "remote", REMOTE),
        "npc_droid_seeker" => droid(&mut choice, "seeker", SEEKER),
        "npc_droid_sentry" => droid(&mut choice, "sentry", SENTRY),
        "npc_droid_gonk" => droid(&mut choice, "gonk", GONK),
        "npc_droid_mouse" => droid(&mut choice, "mouse", MOUSE),
        "npc_droid_r2d2" => droid(
            &mut choice,
            if flags & 1 != 0 { "r2d2_imp" } else { "r2d2" },
            R2D2,
        ),
        "npc_droid_r5d2" => droid(
            &mut choice,
            if flags & 1 != 0 { "r5d2_imp" } else { "r5d2" },
            R5D2,
        ),
        "npc_droid_protocol" => droid(
            &mut choice,
            if flags & 1 != 0 {
                "protocol_imp"
            } else {
                "protocol"
            },
            PROTOCOL,
        ),
        // Every class that keeps a type the map named.
        "npc_reborn_new"
        | "npc_cultist_saber"
        | "npc_cultist_saber_powers"
        | "npc_cultist"
        | "npc_cultist_commando"
        | "npc_jedi"
        | "npc_prisoner"
        | "npc_rebel"
        | "npc_human_merc"
        | "npc_ugnaught"
        | "npc_jawa"
        | "npc_gran"
        | "npc_rodian"
        | "npc_weequay"
        | "npc_trandoshan"
        | "npc_tusken"
        | "npc_noghri"
        | "npc_swamptrooper"
        | "npc_imperial"
        | "npc_impworker"
        | "npc_bespincop"
        | "npc_reborn"
        | "npc_colombian_rebel"
        | "npc_colombian_soldier" => {}
        _ => return None,
    }
    Some(choice)
}

/// A class's own precache by the name `NPC_SpawnType` knows it by (`gonk`, `mouse`,
/// `r2d2`, `atst`, `r5d2`, `mark1`, `mark2`, `interrogator`, `probe`, `seeker`, `remote`,
/// `shadowtrooper`, `minemonster`, `sentry`, `protocol`, `galak_mech`, `wampa`); nothing
/// for any other.
pub fn class_precache(name: &str) -> &'static [Registration] {
    match name {
        "gonk" => GONK,
        "mouse" => MOUSE,
        "r2d2" => R2D2,
        "atst" => ATST,
        "r5d2" => R5D2,
        "mark1" => MARK1,
        "mark2" => MARK2,
        "interrogator" => INTERROGATOR,
        "probe" => PROBE,
        "seeker" => SEEKER,
        "remote" => REMOTE,
        "shadowtrooper" => SHADOWTROOPER,
        "minemonster" => MINEMONSTER,
        "sentry" => SENTRY,
        "protocol" => PROTOCOL,
        "galak_mech" => GALAKMECH,
        "wampa" => WAMPA,
        _ => &[],
    }
}

/// A droid: its type, and its class's precache after the spawner.
fn droid(choice: &mut ClassChoice, name: &str, after: &'static [Registration]) {
    set(&mut choice.npc_type, name);
    choice.after = after;
}

/// `SP_NPC_Cultist_Saber` and `SP_NPC_Cultist_Saber_Powers` (`NPC_spawn.c:2524-2660`):
/// by the flags, medium, strong or all styles, each with a throwing version; the plain
/// throwing powers cultist is the plain one's.
fn cultist_saber(npc_type: &mut Option<Vec<u8>>, flags: i32, powers: bool) {
    let suffix = if powers { "2" } else { "" };
    let style = [(1, "_med"), (2, "_strong"), (4, "_all")]
        .into_iter()
        .find(|(bit, _)| flags & bit != 0)
        .map(|(_, style)| style);
    let throw = flags & 8 != 0;
    let name = match (style, throw) {
        (Some(style), true) => format!("cultist_saber{style}_throw{suffix}"),
        (Some(style), false) => format!("cultist_saber{style}{suffix}"),
        (None, true) => "cultist_saber_throw".to_owned(),
        (None, false) => format!("cultist_saber{suffix}"),
    };
    *npc_type = Some(name.into_bytes());
}

/// A spawner as `SP_NPC_spawner` leaves it: the keys `G_ParseField` read and the ones it
/// read itself.
#[derive(Clone, Debug, PartialEq)]
pub struct NpcSpawner {
    /// The classname, as the map wrote it.
    pub classname: String,
    pub npc_type: Option<Vec<u8>>,
    /// `NPC_Vehicle`.
    pub vehicle: bool,
    /// `s.origin`, `s.angles`.
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub spawnflags: i32,
    /// How many NPCs it has left to spawn (`-1`: for ever).
    pub count: i32,
    /// Milliseconds before a blocked NPC tries again; below zero, it gives up.
    pub wait: f32,
    /// Milliseconds between being used and spawning.
    pub delay: i32,
    /// The NPC's health, where the map gives one.
    pub health: i32,
    /// The `SVF_NO_*_SOUNDS` flags its keys set.
    pub sound_flags: u32,
    /// `showhealth`: `s.shouldtarget`.
    pub shows_health: bool,
    /// `teamowner` (`s.teamowner`), `teamuser`/`alliedteam` (`alliedTeam`), `teamnodmg`,
    /// and `team`.
    pub team_owner: i32,
    pub allied_team: i32,
    pub team_no_damage: i32,
    pub team: Option<Vec<u8>>,
    pub full_name: Vec<u8>,
    pub targetname: Option<Vec<u8>>,
    pub target: Option<Vec<u8>>,
    /// `target2` (also `NPC_target2`), `target3`, `target4` (also `NPC_target4`).
    pub target2: Option<Vec<u8>>,
    pub target3: Option<Vec<u8>>,
    pub target4: Option<Vec<u8>>,
    pub npc_targetname: Option<Vec<u8>>,
    pub npc_target: Option<Vec<u8>>,
    pub close_target: Option<Vec<u8>>,
    pub open_target: Option<Vec<u8>>,
    pub pain_target: Option<Vec<u8>>,
    /// `message`: a key the NPC carries.
    pub message: Option<Vec<u8>>,
    /// `spawnscript`: the ICARUS script it runs once spawned.
    pub spawn_script: Option<Vec<u8>>,
    /// When its think (`NPC_Spawn_Go`) comes, if it has one.
    pub spawn_at: Option<i32>,
    /// `use`: `NPC_Spawn` while it has NPCs left and a name to be used by.
    pub usable: bool,
    /// When it frees itself (`think = G_FreeEntity`): a command's spawner.
    pub free_at: Option<i32>,
    /// What an `NPC_Vehicle` spawner reads besides ([`crate::vehicle_spawn`]).
    pub vehicle_keys: crate::vehicle_spawn::VehicleKeys,
}

/// `G_ParseField`'s `F_STRING`: the value through `G_NewString`.
fn string(entity: &sjk_entity::Entity, key: &str) -> Option<Vec<u8>> {
    entity
        .get(key)
        .map(|value| crate::npc_parms::new_string(value.as_bytes()))
}

/// `F_VECTOR`: `sscanf("%f %f %f")`, what is not read left at zero.
fn vector(value: &str) -> [f32; 3] {
    let mut out = [0.0; 3];
    for (slot, word) in out.iter_mut().zip(value.split_ascii_whitespace()) {
        *slot = atof(word.as_bytes());
    }
    out
}

impl NpcSpawner {
    /// `G_SpawnGEntityFromSpawnVars`' fields and `SP_NPC_spawner` (`NPC_spawn.c:1955-2049`)
    /// for a map entity whose class chose `choice`, at `level_time`: every key read, the
    /// count at least one, `wait` and `delay` from seconds to milliseconds (`wait` a half
    /// second when not given), the sound keys, `showhealth`; and a spawner nobody uses
    /// set to spawn [`AUTO_SPAWN_DELAY`] on. `None` where `g_allowNPC` is off (the spawner
    /// frees itself). The precache is the caller's
    /// ([`crate::npc_precache::spawner_precache`]).
    pub fn from_entity(
        entity: &sjk_entity::Entity,
        choice: &ClassChoice,
        allow: bool,
        level_time: i32,
    ) -> Option<Self> {
        // `SP_NPC_Vehicle` does not ask `g_allowNPC`.
        if !allow && !choice.vehicle {
            return None;
        }
        let int = |key: &str| entity.get(key).map_or(0, |value| atoi(value.as_bytes()));
        let float = |key: &str| entity.get(key).map_or(0.0, |value| atof(value.as_bytes()));
        // `angle` and `angles` both write `s.angles`: whichever the map gave last.
        let angles = entity.fields().iter().rev().find_map(|(key, value)| {
            if key.eq_ignore_ascii_case("angles") {
                Some(vector(value))
            } else if key.eq_ignore_ascii_case("angle") {
                Some([0.0, atof(value.as_bytes()), 0.0])
            } else {
                None
            }
        });
        let full_name = string(entity, "fullname")
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| b"Humanoid Lifeform".to_vec());
        let count = match int("count") {
            0 => 1,
            count => count,
        };
        let sound_flags = SOUND_KEYS
            .iter()
            .filter(|(key, _)| int(key) != 0)
            .fold(0, |flags, (_, flag)| flags | flag);
        let wait = match float("wait") {
            0.0 => DEFAULT_WAIT,
            wait => wait * 1000.0,
        };
        // `delay` is an integer field: `delay *= 1000` in integers.
        let delay = int("delay").wrapping_mul(1000);
        // `teamuser` and `alliedteam` both write `alliedTeam`.
        let allied_team = entity.fields().iter().rev().find(|(key, _)| {
            key.eq_ignore_ascii_case("teamuser") || key.eq_ignore_ascii_case("alliedteam")
        });
        let targetname = string(entity, "targetname");
        let usable = targetname.is_some();
        let mut spawner = Self {
            classname: entity.classname().unwrap_or_default().to_owned(),
            npc_type: choice.npc_type.clone(),
            vehicle: choice.vehicle,
            origin: entity.get("origin").map_or([0.0; 3], vector),
            angles: angles.unwrap_or([0.0; 3]),
            spawnflags: choice.spawnflags,
            count,
            wait,
            delay,
            health: int("health"),
            sound_flags,
            shows_health: int("showhealth") != 0,
            team_owner: int("teamowner"),
            allied_team: allied_team.map_or(0, |(_, value)| atoi(value.as_bytes())),
            team_no_damage: int("teamnodmg"),
            team: string(entity, "team"),
            full_name,
            targetname,
            target: string(entity, "target"),
            target2: last_of(entity, &["target2", "npc_target2"]),
            target3: string(entity, "target3"),
            target4: last_of(entity, &["target4", "npc_target4"]),
            npc_targetname: string(entity, "npc_targetname"),
            npc_target: string(entity, "npc_target"),
            close_target: string(entity, "closetarget"),
            open_target: string(entity, "opentarget"),
            pain_target: string(entity, "paintarget"),
            message: string(entity, "message"),
            spawn_script: string(entity, "spawnscript"),
            spawn_at: (!usable).then_some(level_time + AUTO_SPAWN_DELAY),
            usable,
            free_at: None,
            vehicle_keys: Default::default(),
        };
        if choice.vehicle {
            crate::vehicle_spawn::vehicle_spawner(&mut spawner, entity);
        }
        Some(spawner)
    }

    /// The spawner `npc spawn` makes (`NPC_SpawnType`, `NPC_spawn.c:3953-4099`): at
    /// `origin` facing `yaw`, one NPC, no delay; spawned at once, freed at the next frame.
    pub fn for_command(
        npc_type: &[u8],
        targetname: Option<&[u8]>,
        origin: [f32; 3],
        yaw: f32,
        vehicle: bool,
    ) -> Self {
        Self {
            classname: if vehicle { "NPC_Vehicle" } else { "noclass" }.to_owned(),
            npc_type: Some(crate::npc_parms::new_string(npc_type)),
            vehicle,
            origin,
            angles: [0.0, yaw, 0.0],
            spawnflags: 0,
            count: 1,
            wait: 0.0,
            delay: 0,
            health: 0,
            sound_flags: 0,
            shows_health: false,
            team_owner: 0,
            allied_team: 0,
            team_no_damage: 0,
            team: None,
            full_name: Vec::new(),
            targetname: None,
            target: None,
            target2: None,
            target3: None,
            target4: None,
            npc_targetname: targetname.map(crate::npc_parms::new_string),
            npc_target: None,
            close_target: None,
            open_target: None,
            pain_target: None,
            message: None,
            spawn_script: None,
            spawn_at: None,
            usable: false,
            free_at: None,
            vehicle_keys: Default::default(),
        }
    }
}

/// The last of several keys that write the same field.
fn last_of(entity: &sjk_entity::Entity, keys: &[&str]) -> Option<Vec<u8>> {
    entity
        .fields()
        .iter()
        .rev()
        .find(|(key, _)| keys.iter().any(|known| key.eq_ignore_ascii_case(known)))
        .map(|(_, value)| crate::npc_parms::new_string(value.as_bytes()))
}
