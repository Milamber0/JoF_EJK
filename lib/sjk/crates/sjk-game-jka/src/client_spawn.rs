//! The state a player spawns with: `ClientSpawn` (OpenJK `codemp/game/g_client.c`) for a
//! human player outside siege and Jedi-versus-mercenary, up to the point where the
//! reference runs the spawn's own `ClientThink`. Held against the wire-field dumps of
//! `tools/game-oracle/begin.c`.
//!
//! What this is not: choosing the spawn point, telefragging whoever stands there, the
//! spawn's temp entity, the saber entity, siege classes, the lone power-duelist's
//! health, saber definitions' style restrictions.

use crate::force_config::{FORCE_POWERS, ForceInitialisation, ForceServerSettings};
use crate::pmove::MovementState;
use crate::pmove_anim::{
    AnimationLengths, SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_HOLDLESS,
    SETANIM_FLAG_OVERRIDE, SETANIM_TORSO, set_animation,
};
use sjk_protocol::PlayerState;

const GT_HOLOCRON: i32 = 1;
const GT_JEDI_MASTER: i32 = 2;
const GT_DUEL: i32 = 3;
const GT_POWER_DUEL: i32 = 4;
const GT_SIEGE: i32 = 7;
const TEAM_SPECTATOR: i32 = 3;
const WP_MELEE: u32 = 2;
const WP_SABER: u32 = 3;
const WP_BRYAR_PISTOL: u32 = 4;
const FP_SABER_OFFENSE: usize = 15;
const FP_SABER_DEFENSE: usize = 16;
const BOTH_STAND1TO2: u16 = 927;
const TORSO_RAISEWEAP1: u16 = 1_398;
const EF_TELEPORT_BIT: u32 = 1 << 3;
const EF_INVULNERABLE: u32 = 1 << 27;
const PMF_TIME_KNOCKBACK: u32 = 64;
const PMF_RESPAWNED: u32 = 512;
const ENTITY_NONE: u32 = 1_023;
const WEAPON_RAISING: u32 = 1;
const STAT_HEALTH: usize = 0;
const STAT_WEAPONS: usize = 4;
const STAT_ARMOR: usize = 5;
const STAT_MAX_HEALTH: usize = 8;
const PERS_TEAM: usize = 3;
/// `PERS_SPAWN_COUNT`.
pub const PERS_SPAWN_COUNT: usize = 4;
/// `AMMO_BLASTER`: "100 seems fair".
const AMMO_BLASTER: usize = 2;

/// The saber style fields `ClientSpawn` settles, and the session's memory of them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SaberStyle {
    /// `fd.saberAnimLevel`.
    pub level: i32,
    /// `fd.saberDrawAnimLevel`.
    pub draw_level: i32,
    /// `sess.saberLevel`.
    pub session_level: i32,
}

/// Which sabers the player holds, as far as its style depends on them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SaberKit {
    /// One saber held in one hand.
    #[default]
    Single,
    /// One saber that takes both hands: the staff style.
    Staff,
    /// A saber in each hand.
    Dual,
}

impl SaberStyle {
    /// `ClientSpawn`'s first block, when the player's sabers were (re)set on this spawn:
    /// two sabers and the staff bring their own style; a single saber takes the
    /// session's, 1 to 3, limited by the player's saber attack level.
    pub fn sabers_changed(&mut self, kit: SaberKit, attack_level: i32) {
        match kit {
            SaberKit::Dual => (self.level, self.draw_level) = (6, 6),
            SaberKit::Staff => (self.level, self.draw_level) = (7, 7),
            SaberKit::Single => {
                self.session_level = self.session_level.clamp(1, 3);
                if self.session_level > attack_level {
                    self.session_level = attack_level;
                }
                (self.level, self.draw_level) = (self.session_level, self.session_level);
            }
        }
    }

    /// `WP_InitForcePowers`' first lines: the style is the session's, 1 to 3.
    pub fn force_initialised(&mut self) {
        self.level = if (1..=3).contains(&self.session_level) {
            self.session_level
        } else {
            1
        };
    }

    /// `ClientSpawn`'s third block: a single-saber style that is settled (shown, drawn
    /// and remembered alike) is clamped and limited by the attack level once more.
    pub fn settle(&mut self, attack_level: i32) {
        if !matches!(self.level, 6 | 7)
            && self.level == self.draw_level
            && self.level == self.session_level
        {
            self.session_level = self.session_level.clamp(1, 3);
            if self.session_level > attack_level {
                self.session_level = attack_level;
            }
            (self.level, self.draw_level) = (self.session_level, self.session_level);
        }
    }
}

/// What `ClientSpawn` is given.
#[derive(Clone, Debug)]
pub struct SpawnRequest<'a> {
    /// The wire client number.
    pub client: u16,
    /// `sess.sessionTeam`.
    pub team: i32,
    /// `level.time`.
    pub level_time: i32,
    /// Where the spawn point put the player, and how it faces.
    pub origin: [f32; 3],
    /// Pitch, yaw, roll in degrees.
    pub angles: [f32; 3],
    /// The angles of the client's last command (`pers.cmd.angles`), which the view is
    /// expressed against.
    pub command_angles: [i32; 3],
    /// `pers.maxHealth`, from the handicap.
    pub max_health: i32,
    /// `ps.customRGBA`.
    pub custom_rgba: [u8; 4],
    /// What `WP_InitForcePowers` decided.
    pub force: &'a ForceInitialisation,
    /// The saber style, already through [`SaberStyle`]'s blocks.
    pub saber_style: SaberStyle,
    /// The server's settings.
    pub settings: ForceServerSettings,
    /// `g_spawnInvulnerability` in milliseconds; 0 for none.
    pub spawn_invulnerability: i32,
    /// `ps.eFlags` before this spawn: its teleport bit is toggled.
    pub previous_entity_flags: u32,
    /// `ps.persistant` before this spawn; team and spawn count are rewritten.
    pub persistant: [i32; 16],
    /// `ps.eventSequence`, which survives.
    pub event_sequence: u32,
    /// `ps.saberEntityNum`, which survives.
    pub saber_entity: u32,
}

/// The weapons a player spawns with, and the one in hand (`g_client.c:3380-3520`).
fn weapons(request: &SpawnRequest<'_>) -> (u32, u32) {
    let (gametype, disabled) = (request.settings.gametype, request.settings.weapons_disabled);
    let mut owned = 1; // WP_NONE
    if gametype == GT_HOLOCRON || request.force.levels[FP_SABER_OFFENSE] != 0 {
        owned |= 1 << WP_SABER;
    } else {
        // "if you don't have saber attack rank then you don't get a saber"
        owned |= 1 << WP_MELEE;
    }
    // Siege gives no pistol here: a class hands out its own weapons (`g_client.c:3490`).
    if gametype != GT_SIEGE
        && (disabled & (1 << WP_BRYAR_PISTOL) == 0 || gametype == GT_JEDI_MASTER)
    {
        owned |= 1 << WP_BRYAR_PISTOL;
    }
    if gametype == GT_JEDI_MASTER {
        owned = owned & !(1 << WP_SABER) | 1 << WP_MELEE;
    }
    let held = [WP_SABER, WP_BRYAR_PISTOL]
        .into_iter()
        .find(|weapon| owned & (1 << weapon) != 0)
        .unwrap_or(WP_MELEE);
    (owned, held)
}

/// `WeaponReadyAnim` for the two weapons a player can spawn holding besides the saber.
fn ready_pose(weapon: u32) -> u16 {
    crate::pmove_locomotion::weapon_ready_torso(weapon as u8)
}

/// `ClientSpawn`: the player's state as the spawn leaves it, before the spawn's own
/// `ClientThink` drops it to the floor.
pub fn client_spawn(request: &SpawnRequest<'_>, lengths: &dyn AnimationLengths) -> PlayerState {
    let mut player = PlayerState::zero();
    let spectator = request.team == TEAM_SPECTATOR;
    let mut set = |index: usize, value: u32| {
        player.set_raw_field(index, value);
    };
    for (index, part) in [29, 42, 45, 32].into_iter().zip(request.custom_rgba) {
        set(index, u32::from(part));
    }
    set(31, request.saber_entity);
    set(44, ENTITY_NONE); // duelIndex
    set(39, 100); // jetpackFuel
    set(40, 100); // cloakFuel
    set(19, request.event_sequence);
    let mut flags = (request.previous_entity_flags & EF_TELEPORT_BIT) ^ EF_TELEPORT_BIT;
    set(16, ENTITY_NONE); // groundEntityNum
    set(36, 16); // crouchheight
    set(35, 40); // standheight
    set(43, u32::from(request.client));
    // The Force: what `WP_InitForcePowers` decided, reset by `WP_SpawnInitForcePowers`.
    let mut levels = request.force.levels;
    if request.settings.gametype == GT_HOLOCRON {
        // Powers come from holocrons; a saber-only server still grants the saber's.
        levels = [0; FORCE_POWERS];
        if request.settings.saber_only() {
            (levels[FP_SABER_OFFENSE], levels[FP_SABER_DEFENSE]) = (1, 1);
        }
    }
    let known = (0..FORCE_POWERS)
        .filter(|&power| levels[power] != 0)
        .fold(0_u32, |known, power| known | 1 << power)
        & request.force.known;
    set(18, 100); // fd.forcePower
    set(51, known);
    set(52, u32::from(levels[1])); // FP_LEVITATION
    set(109, u32::from(levels[14])); // FP_SEE
    set(54, request.force.selected as u32);
    set(61, request.force.side as u32);
    set(23, request.saber_style.level as u32);
    set(25, request.saber_style.draw_level as u32);
    set(24, ENTITY_NONE); // rocketLockIndex
    set(26, u32::MAX); // genericEnemyIndex: -1
    let (owned, held) = weapons(request);
    set(47, held);
    set(1, request.origin[1].to_bits());
    set(2, request.origin[0].to_bits());
    set(5, request.origin[2].to_bits());
    // `SetClientViewAngle`: the view, as a difference to the client's last command.
    for (axis, (angle_field, delta_field)) in [(4, 14), (3, 11), (50, 48)].into_iter().enumerate() {
        let short = (request.angles[axis] * 65_536.0 / 360.0) as i32 & 65_535;
        set(angle_field, request.angles[axis].to_bits());
        set(delta_field, (short - request.command_angles[axis]) as u32);
    }
    set(38, PMF_RESPAWNED | PMF_TIME_KNOCKBACK);
    set(41, 100); // pm_time
    if !spectator {
        // The weapon comes up: the saber on the whole body, a gun on the torso over
        // legs already in its ready pose.
        let mut pose = MovementState::default();
        let hold = SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD | SETANIM_FLAG_HOLDLESS;
        if held == WP_SABER {
            set_animation(&mut pose, SETANIM_BOTH, BOTH_STAND1TO2, hold, lengths);
        } else {
            set_animation(&mut pose, SETANIM_TORSO, TORSO_RAISEWEAP1, hold, lengths);
            pose.legs_anim = ready_pose(held);
        }
        for (index, value) in [
            (13, u32::from(pose.legs_anim)),
            (15, u32::from(pose.torso_anim)),
            (21, pose.legs_timer as u32),
            (20, pose.torso_timer as u32),
            (33, WEAPON_RAISING),
            (10, pose.torso_timer as u32),
        ] {
            set(index, value);
        }
        if request.spawn_invulnerability != 0 {
            flags |= EF_INVULNERABLE;
        }
    }
    set(17, flags);
    set(0, (request.level_time - 100) as u32); // commandTime: the spawn's think covers 100 ms
    for (index, value) in request.persistant.iter().enumerate() {
        player.persistent[index] = *value as u32;
    }
    player.persistent[PERS_SPAWN_COUNT] = (request.persistant[PERS_SPAWN_COUNT] + 1) as u32;
    player.persistent[PERS_TEAM] = request.team as u32;
    let duel = matches!(request.settings.gametype, GT_DUEL | GT_POWER_DUEL);
    let max_health = request.max_health;
    // "only start with 100 health in Duel", and no armor; elsewhere a quarter more of each.
    let (health, maximum, armor) = if duel {
        (100, 100, 0)
    } else if max_health <= 100 {
        (
            (max_health as f32 * 1.25) as i32,
            max_health,
            (max_health as f32 * 0.25) as i32,
        )
    } else {
        (
            max_health.max(125),
            max_health,
            (max_health as f32 * 0.25) as i32,
        )
    };
    player.stats[STAT_HEALTH] = health as u32;
    player.stats[STAT_MAX_HEALTH] = maximum as u32;
    player.stats[STAT_ARMOR] = armor as u32;
    player.stats[STAT_WEAPONS] = if spectator { 0 } else { owned };
    player.ammo[AMMO_BLASTER] = 100;
    player
}
