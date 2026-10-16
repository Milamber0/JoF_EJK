//! The `set` types whose multiplayer behaviour is a message and nothing else, and the
//! ones only an NPC takes (`g_ICARUScb.c:2599-5865`), as data: the reference's many
//! one-line setters differ only in their text.

use sjk_icarus::DebugLevel;

use crate::icarus_set_table::*;

/// When the reference calls the setter for a boolean value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Calls {
    /// Always (`"true"` sets, anything else clears; or not a boolean).
    Always,
    /// Only for `"true"` or `"false"` (`if ... else if ...`).
    TrueOrFalse,
}

/// A set type that prints a warning in multiplayer and does nothing more.
pub struct Unsupported {
    pub set: i32,
    pub level: DebugLevel,
    pub message: &'static str,
    pub calls: Calls,
}

const fn warn(set: i32, message: &'static str) -> Unsupported {
    Unsupported {
        set,
        level: DebugLevel::Warning,
        message,
        calls: Calls::Always,
    }
}

/// Set types that only print (`... NOT SUPPORTED IN MP`), in `Q3_Set`'s order.
pub const UNSUPPORTED: &[Unsupported] = &[
    warn(
        SET_PLAYER_TEAM,
        "Q3_SetPlayerTeam: Not in MP ATM, let a programmer (ideally Rich) know if you need it\n",
    ),
    warn(SET_ENEMY_TEAM, "Q3_SetEnemyTeam: NOT SUPPORTED IN MP\n"),
    warn(
        SET_EVENT,
        "Q3_SetEvent: NOT SUPPORTED IN MP (may be in future, ask if needed)\n",
    ),
    warn(
        SET_VIEWENTITY,
        "Q3_SetViewEntity currently unsupported in MP, ask if you need it.\n",
    ),
    warn(SET_ITEM, "Q3_SetItem: NOT SUPPORTED IN MP\n"),
    // (sic: no line break)
    Unsupported {
        set: SET_IGNOREENEMIES,
        level: DebugLevel::Warning,
        message: "Q3_SetIgnoreEnemies: NOT SUPPORTED IN MP",
        calls: Calls::TrueOrFalse,
    },
    Unsupported {
        set: SET_LOCKED_ENEMY,
        level: DebugLevel::Warning,
        message: "Q3_SetLockedEnemy: NOT SUPPORTED IN MP\n",
        calls: Calls::TrueOrFalse,
    },
    warn(SET_LEAN, "SET_LEAN NOT SUPPORTED IN MP\n"),
    warn(SET_TARGET2, "Q3_SetTarget2 does not exist in MP\n"),
    warn(SET_LOCATION, "Q3_SetLocation: NOT SUPPORTED IN MP\n"),
    warn(SET_PAINTARGET, "Q3_SetPainTarget: NOT SUPPORTED IN MP\n"),
    warn(SET_DEFEND_TARGET, "Q3_SetDefendTarget unimplemented\n"),
    warn(SET_NO_MINDTRICK, "Q3_SetNoMindTrick: NOT SUPPORTED IN MP\n"),
    warn(
        SET_CINEMATIC_SKIPSCRIPT,
        "Q3_SetCinematicSkipScript: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_DELAYSCRIPTTIME,
        "Q3_SetDelayScriptTime: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_USE_SUBTITLES,
        "Q3_SetUseSubtitles: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_DISMEMBERABLE,
        "Q3_SetDismemberable: NOT SUPPORTED IN MP\n",
    ),
    warn(SET_MORELIGHT, "Q3_SetMoreLight: NOT SUPPORTED IN MP\n"),
    Unsupported {
        set: SET_TREASONED,
        level: DebugLevel::Verbose,
        message: "SET_TREASONED is disabled, do not use\n",
        calls: Calls::Always,
    },
    warn(SET_VAMPIRE, "Q3_SetVampire: NOT SUPPORTED IN MP\n"),
    warn(
        SET_FORCE_INVINCIBLE,
        "Q3_SetForceInvicible: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_PLAYER_LOCKED,
        "Q3_SetPlayerLocked: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_LOCK_PLAYER_WEAPONS,
        "Q3_SetLockPlayerWeapons: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_NO_IMPACT_DAMAGE,
        "Q3_SetNoImpactDamage: NOT SUPPORTED IN MP\n",
    ),
    warn(SET_CAMERA_GROUP, "Q3_CameraGroup: NOT SUPPORTED IN MP\n"),
    warn(
        SET_CAMERA_GROUP_Z_OFS,
        "Q3_CameraGroupZOfs: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_CAMERA_GROUP_TAG,
        "Q3_CameraGroupTag: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_ADDRHANDBOLT_MODEL,
        "Q3_AddRHandModel: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_REMOVERHANDBOLT_MODEL,
        "Q3_RemoveRHandModel: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_ADDLHANDBOLT_MODEL,
        "Q3_AddLHandModel: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_REMOVELHANDBOLT_MODEL,
        "Q3_RemoveLHandModel: NOT SUPPORTED IN MP\n",
    ),
    warn(SET_FACEEYESCLOSED, "Q3_Face: NOT SUPPORTED IN MP\n"),
    warn(SET_FACEEYESOPENED, "Q3_Face: NOT SUPPORTED IN MP\n"),
    warn(SET_FACEAUX, "Q3_Face: NOT SUPPORTED IN MP\n"),
    warn(SET_FACEBLINK, "Q3_Face: NOT SUPPORTED IN MP\n"),
    warn(SET_FACEBLINKFROWN, "Q3_Face: NOT SUPPORTED IN MP\n"),
    warn(SET_FACEFROWN, "Q3_Face: NOT SUPPORTED IN MP\n"),
    warn(SET_FACENORMAL, "Q3_Face: NOT SUPPORTED IN MP\n"),
    warn(SET_SCROLLTEXT, "Q3_ScrollText: NOT SUPPORTED IN MP\n"),
    // (sic: `Q3_LCARSText` prints the scroll text's name)
    warn(SET_LCARSTEXT, "Q3_ScrollText: NOT SUPPORTED IN MP\n"),
    warn(
        SET_CAPTIONTEXTCOLOR,
        "Q3_SetTextColor: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_CENTERTEXTCOLOR,
        "Q3_SetTextColor: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_SCROLLTEXTCOLOR,
        "Q3_SetTextColor: NOT SUPPORTED IN MP\n",
    ),
    warn(SET_STARTFRAME, "Q3_SetStartFrame: NOT SUPPORTED IN MP\n"),
    warn(SET_LOOP_ANIM, "Q3_SetLoopAnim: NOT SUPPORTED IN MP\n"),
    warn(SET_INTERFACE, "Q3_SetInterface: NOT SUPPORTED IN MP\n"),
    warn(SET_SHIELDS, "Q3_SetShields: NOT SUPPORTED IN MP\n"),
    warn(
        SET_ADJUST_AREA_PORTALS,
        "Q3_SetAdjustAreaPortals: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_DMG_BY_HEAVY_WEAP_ONLY,
        "Q3_SetDmgByHeavyWeapOnly: NOT SUPPORTED IN MP\n",
    ),
    warn(SET_SHIELDED, "Q3_SetShielded: NOT SUPPORTED IN MP\n"),
    warn(SET_NO_GROUPS, "Q3_SetNoGroups: NOT SUPPORTED IN MP\n"),
    warn(
        SET_END_SCREENDISSOLVE,
        "SET_END_SCREENDISSOLVE: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_MISSION_STATUS_SCREEN,
        "SET_MISSION_STATUS_SCREEN: NOT SUPPORTED IN MP\n",
    ),
    warn(SET_VIDEO_PLAY, "SET_VIDEO_PLAY: NOT SUPPORTED IN MP\n"),
    warn(
        SET_VIDEO_FADE_IN,
        "SET_VIDEO_FADE_IN: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_VIDEO_FADE_OUT,
        "SET_VIDEO_FADE_OUT: NOT SUPPORTED IN MP\n",
    ),
    warn(SET_LOADGAME, "SET_LOADGAME: NOT SUPPORTED IN MP\n"),
    warn(
        SET_OBJECTIVE_SHOW,
        "SET_OBJECTIVE_SHOW: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_OBJECTIVE_HIDE,
        "SET_OBJECTIVE_HIDE: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_OBJECTIVE_SUCCEEDED,
        "SET_OBJECTIVE_SUCCEEDED: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_OBJECTIVE_FAILED,
        "SET_OBJECTIVE_FAILED: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_OBJECTIVE_CLEARALL,
        "SET_OBJECTIVE_CLEARALL: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_MISSIONFAILED,
        "SET_MISSIONFAILED: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_MISSIONSTATUSTEXT,
        "SET_MISSIONSTATUSTEXT: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_MISSIONSTATUSTIME,
        "SET_MISSIONSTATUSTIME: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_CLOSINGCREDITS,
        "SET_CLOSINGCREDITS: NOT SUPPORTED IN MP\n",
    ),
    warn(
        SET_DISABLE_SHADER_ANIM,
        "Q3_SetDisableShaderAnims: NOT SUPPORTED IN MP\n",
    ),
    warn(SET_SHADER_ANIM, "Q3_SetShaderAnim: NOT SUPPORTED IN MP\n"),
    warn(SET_MUSIC_STATE, "Q3_SetMusicState: NOT SUPPORTED IN MP\n"),
    warn(
        SET_CLEAN_DAMAGING_ENTS,
        "Q3_SetCleanDamagingEnts: NOT SUPPORTED IN MP\n",
    ),
    warn(SET_HUD, "SET_HUD: NOT SUPPORTED IN MP\n"),
];

/// How a setter that only an NPC takes refuses anything else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// `WL_ERROR "<name>: '<targetname>' is not an NPC\n"`.
    Plain,
    /// `WL_ERROR "<name>: '<targetname>' is not an NPC!\n"`.
    Exclaimed,
    /// `WL_WARNING "<name>: ent <targetname> is not an NPC!\n"`.
    Warned,
}

/// A set type only an NPC takes: the setter's name, how it refuses, when it is called.
pub struct NpcOnly {
    pub set: i32,
    pub name: &'static str,
    pub refusal: Refusal,
    pub calls: Calls,
}

const fn npc(set: i32, name: &'static str, refusal: Refusal, calls: Calls) -> NpcOnly {
    NpcOnly {
        set,
        name,
        refusal,
        calls,
    }
}

use Calls::{Always, TrueOrFalse};
use Refusal::{Exclaimed, Plain, Warned};

/// The NPC-only set types without a task to wait on.
pub const NPC_ONLY: &[NpcOnly] = &[
    npc(SET_DEFAULT_BSTATE, "Q3_SetDefaultBState", Plain, Always),
    npc(SET_SHOOTDIST, "Q3_SetShootDist", Plain, Always),
    npc(SET_VISRANGE, "Q3_SetVisrange", Plain, Always),
    npc(SET_EARSHOT, "Q3_SetEarshot", Plain, Always),
    npc(SET_VIGILANCE, "Q3_SetVigilance", Plain, Always),
    npc(SET_VFOV, "Q3_SetVFOV", Plain, Always),
    npc(SET_HFOV, "Q3_SetHFOV", Plain, Always),
    npc(SET_WATCHTARGET, "Q3_SetWatchTarget", Exclaimed, Always),
    npc(SET_WALKSPEED, "Q3_SetWalkSpeed", Exclaimed, Always),
    npc(SET_RUNSPEED, "Q3_SetRunSpeed", Exclaimed, Always),
    npc(SET_YAWSPEED, "Q3_SetYawSpeed", Exclaimed, Always),
    npc(SET_AGGRESSION, "Q3_SetAggression", Exclaimed, Always),
    npc(SET_AIM, "Q3_SetAim", Exclaimed, Always),
    npc(SET_SHOT_SPACING, "Q3_SetShotSpacing", Exclaimed, Always),
    npc(SET_FOLLOWDIST, "Q3_SetFollowDist", Exclaimed, Always),
    npc(SET_REMOVE_TARGET, "Q3_SetRemoveTarget", Exclaimed, Always),
    npc(SET_CAPTURE, "Q3_SetCaptureGoal", Exclaimed, Always),
    npc(SET_IGNOREPAIN, "Q3_SetIgnorePain", Exclaimed, TrueOrFalse),
    npc(
        SET_IGNOREALERTS,
        "Q3_SetIgnoreAlerts",
        Exclaimed,
        TrueOrFalse,
    ),
    npc(SET_DONTFIRE, "Q3_SetDontFire", Exclaimed, TrueOrFalse),
    npc(SET_FIRE_WEAPON, "Q3_SetFireWeapon", Exclaimed, TrueOrFalse),
    npc(SET_CROUCHED, "Q3_SetCrouched", Exclaimed, Always),
    npc(SET_WALKING, "Q3_SetWalking", Exclaimed, Always),
    npc(SET_RUNNING, "Q3_SetRunning", Exclaimed, Always),
    npc(SET_CHASE_ENEMIES, "Q3_SetChaseEnemies", Exclaimed, Always),
    npc(
        SET_LOOK_FOR_ENEMIES,
        "Q3_SetLookForEnemies",
        Exclaimed,
        Always,
    ),
    npc(SET_FACE_MOVE_DIR, "Q3_SetFaceMoveDir", Exclaimed, Always),
    npc(SET_ALT_FIRE, "Q3_SetAltFire", Exclaimed, Always),
    npc(SET_DONT_FLEE, "Q3_SetDontFlee", Exclaimed, Always),
    npc(SET_FORCED_MARCH, "Q3_SetForcedMarch", Exclaimed, Always),
    npc(SET_NO_RESPONSE, "Q3_SetNoResponse", Exclaimed, Always),
    npc(SET_NO_COMBAT_TALK, "Q3_SetCombatTalk", Exclaimed, Always),
    npc(SET_NO_ALERT_TALK, "Q3_SetAlertTalk", Exclaimed, Always),
    npc(SET_USE_CP_NEAREST, "Q3_SetUseCpNearest", Exclaimed, Always),
    npc(SET_NO_FORCE, "Q3_SetNoForce", Exclaimed, Always),
    npc(SET_NO_ACROBATICS, "Q3_SetNoAcrobatics", Exclaimed, Always),
    npc(SET_NO_FALLTODEATH, "Q3_SetNoFallToDeath", Exclaimed, Always),
    npc(SET_NOAVOID, "Q3_SetNoAvoid", Exclaimed, Always),
    npc(SET_GREET_ALLIES, "Q3_SetGreetAllies", Warned, Always),
];

/// Set types a client (player or NPC) takes only to print that multiplayer does not
/// support them: the setter's name, then the warning.
pub const CLIENT_UNSUPPORTED: &[(i32, &str, &str)] = &[
    (
        SET_FRICTION,
        "Q3_SetFriction",
        "Q3_SetFriction currently unsupported in MP\n",
    ),
    (
        SET_FORWARDMOVE,
        "Q3_SetForwardMove",
        "Q3_SetForwardMove: NOT SUPPORTED IN MP\n",
    ),
    (
        SET_RIGHTMOVE,
        "Q3_SetRightMove",
        "Q3_SetRightMove: NOT SUPPORTED IN MP\n",
    ),
    (
        SET_LOCKYAW,
        "Q3_SetLockAngle",
        "Q3_SetLockAngle is not currently available. Ask if you really need it.\n",
    ),
];

/// Whether a boolean setter is called for this value.
pub fn called(calls: Calls, value: &str) -> bool {
    match calls {
        Calls::Always => true,
        Calls::TrueOrFalse => {
            value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false")
        }
    }
}
