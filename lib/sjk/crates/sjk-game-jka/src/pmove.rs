//! OpenJK `codemp` compatible local-player movement prediction.
//!
//! This module is the one movement implementation used by both the live client
//! and the parity harness. Legacy constants and state stay in this compatibility
//! adapter rather than leaking into the runtime world model.

use glam::Vec3;
use sjk_protocol::{ENTITY_NUMBER_NONE, PlayerState, UserCommand};

pub const PLAYER_CONTENT_MASK: u32 = 0x0000_0001 | 0x0000_0010 | 0x0000_0100 | 0x0000_1000;
pub const ENTITY_NUMBER_WORLD: u16 = 1_022;

/// `STAT_HOLDABLE_ITEM`, `STAT_HOLDABLE_ITEMS`.
const STAT_HOLDABLE_ITEM: usize = 1;
const STAT_HOLDABLE_ITEMS: usize = 2;
/// `STAT_MAX_HEALTH`.
const STAT_MAX_HEALTH: usize = 8;
const MIN_WALK_NORMAL: f32 = 0.7;
const OVERCLIP: f32 = 1.001;
const STEP_SIZE: f32 = 18.0;
const JUMP_VELOCITY: f32 = 225.0;
const STOP_SPEED: f32 = 100.0;
const GROUND_ACCELERATION: f32 = 10.0;
const AIR_ACCELERATION: f32 = 1.0;
const FRICTION: f32 = 6.0;
/// `codemp/game/surfaceflags.h`: JKA moved SURF_SLICK out of the low
/// material bits. Quake III's 0x2 is NOT a physics flag in protocol-26 maps.
const SURF_SLICK: u32 = 0x0000_4000;
const DUCK_SCALE: f32 = 0.5;
const PMF_DUCKED: u16 = 1;
const PMF_JUMP_HELD: u16 = 2;
const PMF_TIME_LAND: u16 = 32;
const PMF_TIME_KNOCKBACK: u16 = 64;
const PMF_TIME_WATERJUMP: u16 = 256;
const PMF_ALL_TIMES: u16 = PMF_TIME_LAND | PMF_TIME_KNOCKBACK | PMF_TIME_WATERJUMP;

/// `bg_public.h` angle axis order; only pitch is clamped by `PM_UpdateViewAngles`.
const PITCH: usize = 0;
/// `bg_pmove.c` PM_UpdateViewAngles: 16000 short units, 87.89 degrees, "don't let the
/// player look up or down more than 90 degrees".
const PITCH_CLAMP: i16 = 16_000;
pub(crate) const FORCE_LEVITATION_BIT: u32 = 1 << 1;
/// The `eFlags` bits `Pmove` sets and clears: `EF_TALK`, `EF_FIRING`, `EF_ALT_FIRING`,
/// `EF_JETPACK_FLAMING`. Every other bit is the game's.
const PMOVE_ENTITY_FLAGS: u32 = (1 << 13) | (1 << 9) | (1 << 10) | (1 << 30);
/// `PMF_STUCK_TO_WALL`: grabbing a wall.
const PMF_STUCK_TO_WALL: u16 = 16_384;
/// `MAX_CLIENTS`: entity numbers below it are players, to whom `g_stepSlideFix` applies.
const MAX_CLIENTS: u16 = 32;

/// `pml.frametime = pml.msec * 0.001` (`bg_pmove.c:10566`): the product is a *double's*
/// — `0.001` is a double literal — and only the result becomes a float. Multiplying in
/// single precision differs by a unit in the last place for 102 of the first 200
/// millisecond counts (5, 9, 10, 11, 15 ... but not 3, 4, 7 or 8), which is one unit in
/// the last place of a falling player's height after a single 9 ms command.
pub(crate) fn frame_seconds(millis: i32) -> f32 {
    (f64::from(millis) * 0.001) as f32
}
/// `pmtype_t::PM_DEAD` (`codemp/game/bg_public.h`).
const PM_DEAD: u8 = 5;
const PM_NOCLIP: u8 = 3;
/// `DEFAULT_VIEWHEIGHT` = `DEFAULT_MAXS_2 + STANDARD_VIEWHEIGHT_OFFSET` = 40 - 4
/// (`codemp/game/bg_public.h:75-80`; the `//26` comment there is stale).
const STANDING_VIEW_HEIGHT: i32 = 36;
/// `CROUCH_VIEWHEIGHT` = `CROUCH_MAXS_2 + STANDARD_VIEWHEIGHT_OFFSET` = 16 - 4.
const CROUCH_VIEW_HEIGHT: i32 = 12;
/// `DEAD_VIEWHEIGHT` (`bg_public.h:82`).
const DEAD_VIEW_HEIGHT: i32 = -16;
/// `PM_CheckDuck` dead top (`bg_pmove.c:4484-4489`).
const DEAD_TOP: f32 = -8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementTrace {
    pub fraction: f32,
    pub end_position: [f32; 3],
    pub plane_normal: [f32; 3],
    pub surface_flags: u32,
    pub start_solid: bool,
    pub all_solid: bool,
    pub entity_number: u16,
}

impl MovementTrace {
    pub fn miss(end_position: [f32; 3]) -> Self {
        Self {
            fraction: 1.0,
            end_position,
            plane_normal: [0.0; 3],
            surface_flags: 0,
            start_solid: false,
            all_solid: false,
            entity_number: ENTITY_NUMBER_NONE,
        }
    }
}

/// World queries used by `codemp`'s `pmove_t::trace` callback.
/// What the game knows of a player that its move reads beyond the wire state: its saber
/// skills (`fd.forcePowerLevel` of saber offense, defense and throw, which no client is
/// sent), its sabers' definitions, the game type, whether the game follows a thrown saber,
/// and who a trace meets (`PM_BGEntForNum`: the legs of a player or an NPC).
pub struct MoveContext<'a> {
    pub saber_offense: u8,
    pub saber_defense: u8,
    pub saber_throw: u8,
    pub sabers: crate::saber_info::Sabers,
    pub gametype: i32,
    pub saber_throws: bool,
    pub bodies: &'a dyn Fn(u16) -> Option<u16>,
    /// Whether entity `number` is an NPC and no vehicle (`PM_BGEntForNum`'s `s.eType ==
    /// ET_NPC`): what stands on one is bounced off its head.
    pub npcs: &'a dyn Fn(u16) -> bool,
    /// `PM_FootSlopeTrace`'s `G2API_GetBoltMatrix` on the mover's own model (`pm->ghoul2`,
    /// `*l_leg_foot` then `*r_leg_foot`): where its feet are with it standing at `origin`
    /// facing `yaw`, at the model's clock and scale; `None` where it has no humanoid model
    /// (`PM_AdjustStandAnimForSlope` then changes nothing, [`crate::pmove_slope`]).
    pub foot_bolts: &'a FootBolts<'a>,
}

/// Where a mover's feet are ([`MoveContext::foot_bolts`]): left, then right.
pub type FootBolts<'a> = dyn Fn([f32; 3], f32) -> Option<([f32; 3], [f32; 3])> + 'a;

fn no_bodies(_: u16) -> Option<u16> {
    None
}

impl<'a> MoveContext<'a> {
    /// The same context with the mover's feet read by `foot_bolts` (a host that keeps the
    /// mover's model gives its own, [`MoveContext::foot_bolts`]).
    pub fn with_foot_bolts<'b>(&'b self, foot_bolts: &'b FootBolts<'b>) -> MoveContext<'b> {
        MoveContext {
            sabers: self.sabers,
            bodies: self.bodies,
            npcs: self.npcs,
            foot_bolts,
            ..*self
        }
    }
}

/// No model: no feet to read.
pub fn no_foot_bolts(_: [f32; 3], _: f32) -> Option<([f32; 3], [f32; 3])> {
    None
}

fn no_npcs(_: u16) -> bool {
    false
}

impl MoveContext<'static> {
    /// What a client predicting its own move knows: no saber skills, the stock saber, no
    /// throws, nobody met.
    pub const CLIENT: Self = Self {
        saber_offense: 0,
        saber_defense: 0,
        saber_throw: 0,
        sabers: crate::saber_info::Sabers::STOCK,
        gametype: 0,
        saber_throws: false,
        bodies: &no_bodies,
        npcs: &no_npcs,
        foot_bolts: &no_foot_bolts,
    };
}

pub trait MovementCollision {
    /// Contents for PM_SetWaterLevel and ledge probes; dry synthetic worlds may omit it.
    fn point_contents(&self, _point: [f32; 3]) -> u32 {
        0
    }
    fn trace(
        &self,
        start: [f32; 3],
        minimums: [f32; 3],
        maximums: [f32; 3],
        end: [f32; 3],
        content_mask: u32,
    ) -> MovementTrace;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementConfig {
    /// CS_SERVERINFO g_noSpecMove, consumed before spectator flight.
    pub no_spectator_move: bool,
    pub step_slide_fix: bool,
    pub snap_velocity: bool,
    pub fixed_millis: Option<i32>,
    /// Server-dialect roll behavior selected from `CS_SERVERINFO`.
    pub roll_rules: crate::pmove_roll::RollRules,
    /// Equipped saber definitions, applied in primary/secondary order.
    pub saber_speed_scales: [f32; 2],
    /// The sabers' `animSpeedScale` (`BG_SaberStartTransAnim`), primary then secondary;
    /// 1 for a hand without a saber. Copied into [`MovementState`] for the animations.
    pub saber_anim_speed_scales: [f32; 2],
    /// What a client knows of the sabers it holds, to work out `fd.saberAnimLevelBase` for each
    /// command ([`saber_base`]); `None` leaves the state's base alone, as a server does.
    pub saber_hands: Option<saber_base::SaberHands>,
    /// Whose `Pmove` this is. A client's prediction (`false`, the default) also does
    /// what `CG_PredictPlayerState` does around it; a server's simulation (`true`) is
    /// the bare `Pmove` that `ClientThink_real` calls.
    pub authoritative: bool,
    /// `CS_LEGACY_FIXES`: which of the reference's animation fixes the server runs
    /// (`g_fixSaberMoveData`, `g_fixWeaponAttackAnim`, `g_fixRunWalkAnims`, bits 0-2).
    /// A retail server publishes none and runs none.
    pub legacy_fixes: u32,
    /// `CS_SERVERINFO` `g_debugMelee` as the server's dialect reads it. SJK's own server
    /// does not simulate it and keeps the default (off).
    pub debug_melee: crate::pmove_debug_melee::DebugMelee,
    /// The server's grapple-hook pull: JA+ only ([`grapple`]).
    pub grapple: Option<grapple::GrappleRules>,
    /// JA+ server rules selected from `CS_SERVERINFO`; stock on other servers.
    pub ja_plus: crate::pmove_japlus::JaPlusRules,
}

impl Default for MovementConfig {
    fn default() -> Self {
        Self {
            no_spectator_move: false,
            step_slide_fix: true,
            snap_velocity: true,
            fixed_millis: None,
            roll_rules: crate::pmove_roll::RollRules::default(),
            saber_speed_scales: [1.0; 2],
            saber_anim_speed_scales: [1.0; 2],
            saber_hands: None,
            authoritative: false,
            // The reference server's cvar defaults: every fix on.
            legacy_fixes: 0b111,
            debug_melee: crate::pmove_debug_melee::DebugMelee::default(),
            grapple: None,
            ja_plus: crate::pmove_japlus::JaPlusRules::default(),
        }
    }
}

/// The two hands' saber `animSpeedScale`: 1 each unless a definition says otherwise, a
/// default state included.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HiltSpeedScales(pub [f32; 2]);

impl Default for HiltSpeedScales {
    fn default() -> Self {
        Self([1.0; 2])
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MovementState {
    /// Authoritative pickup eligibility fields, separate from predicted grants.
    pub pickup_limits: crate::prediction_items::PickupLimits,
    /// Legacy entity hide mask for this replay; reset on authoritative reseed.
    pub predicted_items: [u64; 16],
    /// Last touched pad in this command, cleared when no pad is touched.
    pub jump_pad_entity: Option<u16>,
    /// Teleport-trigger contact; no speculative teleport destination is invented.
    pub hyperspace: bool,
    /// PM_Footsteps cycle, seeded from the existing protocol field.
    pub bob_cycle: u8,
    /// Next predictable-event slot (wire sequence wraps at 16 bits).
    pub event_sequence: u16,
    pub command_time: i32,
    pub client_num: u16,
    pub movement_type: u8,
    pub movement_flags: u16,
    pub movement_time: i32,
    pub team: u8,
    pub health: i32,
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    pub gravity: f32,
    /// Transient PM_SetWaterLevel result; recomputed from collision contents each move.
    pub water_level: u8,
    /// Complete contents mask at the lowest water sample, not just MASK_WATER bits.
    pub water_type: u32,
    pub speed: f32,
    /// Authoritative base speed restored before compatibility modifiers.
    pub base_speed: f32,
    pub ground_entity_number: u16,
    /// Eight-sector strafe direction published for cgame leg/torso yaw
    /// (`PM_SetMovementDir`).
    pub movement_direction: i8,
    pub view_angles: [f32; 3],
    pub delta_angles: [i32; 3],
    pub view_height: i32,
    pub standing_height: f32,
    pub crouching_height: f32,
    pub force_jump_start_height: f32,
    pub force_power: u8,
    pub levitation_level: u8,
    pub force_powers_active: u32,
    /// `fd.forcePowersKnown`: the powers a command may select.
    pub force_powers_known: u32,
    /// `fd.forcePowerSelected`: the power the Force button uses.
    pub force_power_selected: u8,
    /// `stats[STAT_HOLDABLE_ITEMS]` and `stats[STAT_HOLDABLE_ITEM]`: the items held, by
    /// tag, and the one selected, by its index in the item list.
    pub holdable_items: u32,
    pub holdable_item: u32,
    /// `stats[STAT_MAX_HEALTH]`: a medpac heals up to it (`PM_ItemUsable`).
    pub max_health: i32,
    /// `fd.sentryDeployed`: a second sentry is refused (`PM_ItemUsable`).
    pub sentry_deployed: bool,
    /// `fd.forcePowerDebounce[FP_LEVITATION]`: the command time from which a Force jump
    /// in progress next costs Force.
    pub levitation_debounce: i32,
    pub force_rage_recovery_time: i32,
    /// Networked grip-victim slowdown, separate from the caster's active power.
    pub force_grip_cripple: bool,
    /// Local-only disruptor zoom gate; not a protocol-26 netfield.
    pub zoom_lock_time: i32,
    pub force_hand_extend: u8,
    /// `activeForcePass`: the level of the lightning (or drain) being cast, which the
    /// game sets; two-handed lightning above level 2 with the fists.
    pub active_force_pass: u8,
    /// `forceDodgeAnim`: the pose a dodge or a taunt holds; a knockdown's get-up.
    pub force_dodge_anim: u16,
    /// Whether `PM_Footsteps` selected an airborne animation.
    pub in_air_animation: bool,
    /// The legs timer as the command found it: `PM_DropTimers` runs after the move in
    /// the reference, so `PM_CheckJump`'s `PM_InKnockDown` sees the last command's timer.
    /// Local only.
    pub legs_timer_at_entry: i32,
    /// Local deadline set while a full-body taunt maintains its hand pose.
    pub force_hand_extend_time: i32,
    /// `fd.forceJumpCharge`: a charged Force jump the game gave (an NPC's AI charges them;
    /// a player's is always 0). No wire field: the mover sets it before each move.
    pub force_jump_charge: f32,
    /// `ps.forceJumpFlip`: the game's `ForceJump` began a jump whose flip `PM_CheckJump`
    /// plays at the next move, clearing it. No wire field.
    pub force_jump_flip: bool,
    pub force_restricted: bool,
    pub true_non_jedi: bool,
    pub falling_to_death: i32,
    pub saber_lock_frame: u16,
    pub vehicle_entity_num: u16,
    pub emplaced_index: u16,
    pub zoom_mode: u8,
    /// Networked zoom lock and saved scope values (`bg_pmove.c:8080-8117`).
    pub zoom_locked: bool,
    /// Previous command time at unzoom, not the current slice endpoint.
    pub zoom_time: i32,
    /// Stock server approximation of the held zoom level.
    pub zoom_fov: f32,
    /// Start time of a primary or alternate charge.
    pub weapon_charge_time: i32,
    /// Networked deadline for the next charge ammo subtraction.
    pub weapon_charge_subtract_time: i32,
    /// The rocket launcher's lock (`rocketLockIndex`, `rocketLockTime`, `rocketTargetTime`):
    /// a weapon change clears it (`BG_ClearRocketLock`: no entity, `-1.0`, `0`).
    pub rocket_lock_index: u16,
    pub rocket_lock_time: f32,
    /// `rocketTargetTime`: a float, like the lock's time (`q_shared.h:1188`).
    pub rocket_target_time: f32,
    /// `rocketLastValidTime`, which is not on the wire: the lock's time before it went
    /// off the target.
    pub rocket_last_valid_time: f32,
    /// `footstepTime`, which is not on the wire either: until when the last step is heard
    /// (the bots' hearing reads it).
    pub footstep_time: i32,
    pub forced_saber: bool,
    pub weapon: u8,
    pub weapon_state: u8,
    pub weapon_time: i32,
    /// Current protocol `saberMoveName_t`, retained at its 32-bit wire width.
    pub saber_move: u32,
    /// Server block response passed through until the blocked branch is predicted.
    pub saber_blocked: u8,
    /// Blocking mode selected by the current move-table row.
    pub saber_blocking: u8,
    /// Number of linked attacks in the current kata.
    pub saber_attack_chain_count: u8,
    /// Fast, medium, strong, dual, or staff animation group.
    pub saber_anim_level: u8,
    /// `fd.saberAnimLevelBase`: the style the equipped sabers start from. Not on the
    /// wire, so zero on a client; the game sets it on a server.
    pub saber_anim_level_base: u8,
    /// `slopeRecalcTime`: when legs in a slope pose may step back again. Not on the wire.
    pub slope_recalc_time: i32,
    /// `PM_AdjustStandAnimForSlope` ran this command: a saber carrier standing still
    /// read its foot bolts (`PM_FootSlopeTrace`), which stamps the server's Ghoul2
    /// skeleton cache at its clock. The slope poses themselves are not ported yet.
    pub read_foot_bolts: bool,
    /// `fd.forceJumpSound`: a Force jump began since the game last looked, which the game
    /// sounds (`PDSOUND_FORCEJUMP`) at its next Force update. Not on the wire.
    pub force_jump_sound: bool,
    /// Number of holstered saber blades from player state.
    pub saber_holstered: u8,
    /// Whether the saber entity is currently in flight.
    pub saber_in_flight: bool,
    /// Attached saber entity, required by stock's weapon-switch request gate.
    pub saber_entity_num: u16,
    /// Server time through which a saber lock freezes all movement input.
    pub saber_lock_time: i32,
    /// `saberCanThrow`: the game's leave to throw the saber.
    pub saber_can_throw: bool,
    /// `saberLockEnemy`: whom the lock is with.
    pub saber_lock_enemy: u16,
    /// `saberLockAdvance`: the game's leave to push the lock this move.
    pub saber_lock_advance: bool,
    /// Protocol torso animation ordinal; restart toggling is a separate netfield.
    pub torso_anim: u16,
    pub torso_timer: i32,
    /// Torso restart toggle carried by its own protocol-26 netfield.
    pub torso_flip: bool,
    /// Protocol legs animation ordinal; restart toggling is a separate netfield.
    pub legs_anim: u16,
    /// Remaining hold time for the legs animation.
    pub legs_timer: i32,
    /// Legs restart toggle carried by its own protocol-26 netfield.
    pub legs_flip: bool,
    /// Server side only: the legs and torso animations the player's *entity* shows,
    /// as last copied from this state (`BG_PlayerStateToEntityState`). The game build
    /// of `BG_StartLegsAnim`/`BG_StartTorsoAnim` restarts an animation that was left
    /// and re-entered before the entity caught up. `None` on a client, whose build
    /// has no such rule.
    pub entity_animations: Option<[u16; 2]>,
    /// Broken-limb mask used by codemp's saber animation speed adjustment (and, on a
    /// fighter, its damaged surfaces).
    pub broken_limbs: u8,
    /// `hackingTime`: on a fighter, how long it has strafed which way
    /// ([`crate::vehicle_fighter`]). The game's to write back.
    pub hacking_time: i32,
    /// The sabers' `animSpeedScale` ([`MovementConfig::saber_anim_speed_scales`]).
    pub saber_anim_speed_scales: HiltSpeedScales,
    /// Commands intentionally left to the server-only special-move branches.
    pub saber_special_deferred: u32,
    /// Whether the last command lost nonzero movement to the input-freeze rules.
    pub input_freeze_active: bool,
    /// Whether the last simulated slice held the view in a wall move. Presentation
    /// must keep this view even on frames that cannot advance prediction. Local only.
    pub wall_view_locked: bool,
    /// Whether the last command encountered a deferred saber weapon slice.
    pub saber_deferred_active: bool,
    /// Whether a private duel is active (`bg_pmove.c:6996-7000`).
    pub duel_in_progress: bool,
    /// Server time until which a fresh private duel freezes movement input.
    pub duel_time: i32,
    pub ammo: [i32; 16],
    pub weapons: u32,
    pub entity_flags: u32,
    pub has_detpack_planted: bool,
    /// Protocol powerup expiry times used by `BG_HasYsalamiri`.
    pub powerup_deadlines: [i32; 16],
    /// `lastHitLoc`: where a JA+ grapple hook holds on ([`grapple`]). The game sets it;
    /// the movement only reads it.
    pub last_hit_location: [f32; 3],
}

impl MovementState {
    /// Server side: write what movement owns into a wire player state, the inverse of
    /// [`Self::from_player_state`] for those fields. Everything else in `player` —
    /// stats, weapons, animations — belongs to other parts of the game and is left
    /// alone.
    pub fn write_player_state(&self, player: &mut PlayerState) {
        player.set_command_time(self.command_time);
        player.set_client_num(self.client_num);
        player.set_movement_type(self.movement_type);
        player.set_movement_flags(self.movement_flags);
        player.set_movement_time(self.movement_time as i16);
        player.set_origin(self.origin);
        player.set_velocity(self.velocity);
        player.set_gravity(self.gravity as i32);
        player.set_speed(self.speed);
        // A client's `BG_AdjustClientSpeed` rebuilds `speed` from these on every move.
        player.set_base_speed(self.base_speed as i32);
        // Stats are 16-bit shorts on the wire, read sign-extended: stored that way too.
        player.stats[0] = self.health as i16 as i32 as u32;
        player.persistent[3] = u32::from(self.team);
        player.set_ground_entity_num(self.ground_entity_number);
        player.set_view_angles(self.view_angles);
        player.set_delta_angles(self.delta_angles);
        player.set_view_height(self.view_height);
        // What `Pmove` decides besides motion, by protocol-26 field index: a legacy
        // client draws and hears these.
        for (index, value) in [
            (9, u32::from(self.bob_cycle)),
            (10, self.weapon_time as u32),
            (13, u32::from(self.legs_anim)),
            (15, u32::from(self.torso_anim)),
            (19, u32::from(self.event_sequence)),
            (20, self.torso_timer as u32),
            (21, self.legs_timer as u32),
            (30, self.movement_direction as u32),
            (33, u32::from(self.weapon_state)),
            (34, self.saber_move),
            // `saberAttackChainCount`: how many swings the chain has run to.
            (62, u32::from(self.saber_attack_chain_count)),
            // `saberBlocked`: the game raises the saber, the movement takes it up and
            // clears it.
            (77, u32::from(self.saber_blocked)),
            // `saberInFlight`, `saberCanThrow`: a throw the movement began.
            (88, u32::from(self.saber_in_flight)),
            (49, u32::from(self.saber_can_throw)),
            // `saberHolstered`: an attack with the saber off turns it on (`bg_saber.c:2912`),
            // and a roll's stab too (`:2807`).
            (81, u32::from(self.saber_holstered)),
            (47, u32::from(self.weapon)),
            // `emplacedIndex`: the movement lets go of an emplaced gun (backing off it,
            // leaving the ground, a duel).
            (112, u32::from(self.emplaced_index)),
            // `forceHandExtend`, `forceDodgeAnim`: a pose the game holds the player in,
            // which the movement ends (`HANDEXTEND_WEAPONREADY`).
            (80, u32::from(self.force_hand_extend)),
            (89, u32::from(self.force_dodge_anim)),
            // A saber lock: until when, the frame it is held at, with whom, and the
            // game's leave to push it, which the movement spends.
            (107, self.saber_lock_time as u32),
            (108, u32::from(self.saber_lock_frame)),
            (110, u32::from(self.saber_lock_enemy)),
            (120, u32::from(self.saber_lock_advance)),
            // A charged weapon's charge: when it began and when it next costs
            // (`weaponChargeTime`, `weaponChargeSubtractTime`), which the game's fire
            // reads for the shot's strength.
            (68, self.weapon_charge_time as u32),
            (67, self.weapon_charge_subtract_time as u32),
            (24, u32::from(self.rocket_lock_index)),
            (79, self.rocket_lock_time.to_bits()),
            (71, self.rocket_target_time.to_bits()),
            (55, u32::from(self.torso_flip)),
            (59, u32::from(self.in_air_animation)),
            (69, u32::from(self.legs_flip)),
            // `fd.forceJumpZStart`: where the jump began, which a client's Force jump and
            // its landing both read.
            (74, self.force_jump_start_height.to_bits()),
            // `eFlags`: `Pmove` owns the firing, talking and jetpack bits of the player's
            // flags and leaves the game's alone — the two share one field on the wire.
            (
                17,
                (player.raw_field(17).unwrap_or(0) & !PMOVE_ENTITY_FLAGS)
                    | (self.entity_flags & PMOVE_ENTITY_FLAGS),
            ),
            // What a Force jump costs while it lasts: the pool, when it next pays, and
            // that it is on (`fd.forcePower`, `fd.forcePowerDebounce[FP_LEVITATION]`,
            // `fd.forcePowersActive`).
            (18, u32::from(self.force_power)),
            (53, self.levitation_debounce as u32),
            (82, self.force_powers_active),
            // The power a command selected (`PmoveSingle`'s `forcesel`).
            (54, u32::from(self.force_power_selected)),
            // The disruptor's scope (`zoomMode`, `zoomTime`, `zoomLocked`, `zoomFov`).
            (90, u32::from(self.zoom_mode)),
            (92, self.zoom_time as u32),
            (94, u32::from(self.zoom_locked)),
            (95, self.zoom_fov.to_bits()),
        ] {
            player.set_raw_field(index, value);
        }
        // The item a command selected (`invensel`), and the items the use key used up.
        player.stats[STAT_HOLDABLE_ITEM] = self.holdable_item;
        player.stats[STAT_HOLDABLE_ITEMS] = self.holdable_items;
        // The ammo, which firing and charging spend.
        for (slot, ammo) in player.ammo.iter_mut().zip(self.ammo) {
            *slot = ammo as u32;
        }
    }

    /// Copies the networked subset of `codemp`'s `playerState_t` needed by
    /// on-foot `Pmove`; this is the authoritative replay starting point.
    pub fn from_player_state(player: &PlayerState) -> Self {
        Self {
            pickup_limits: crate::prediction_items::PickupLimits::from_player(player),
            predicted_items: [0; 16],
            jump_pad_entity: None,
            hyperspace: false,
            bob_cycle: player.bob_cycle(),
            event_sequence: player.event_sequence() as u16,
            command_time: player.command_time(),
            client_num: player.client_num(),
            movement_type: player.movement_type(),
            movement_flags: player.movement_flags(),
            movement_time: i32::from(player.movement_time()),
            team: player.team(),
            health: player.health(),
            origin: player.origin(),
            velocity: player.velocity(),
            gravity: player.gravity() as f32,
            water_level: 0,
            water_type: 0,
            speed: player.speed(),
            base_speed: player.raw_field(37).unwrap_or(0) as i32 as f32,
            ground_entity_number: player.ground_entity_num(),
            movement_direction: player.movement_direction(),
            view_angles: player.view_angles(),
            delta_angles: player.delta_angles(),
            view_height: player.view_height(),
            standing_height: f32::from(player.standing_height()),
            crouching_height: f32::from(player.crouching_height()),
            force_jump_start_height: player.force_jump_start_height(),
            force_power: player.force_power(),
            levitation_level: player.levitation_level().min(3),
            force_powers_active: player.force_powers_active(),
            force_powers_known: player.raw_field(51).unwrap_or(0),
            force_power_selected: player.raw_field(54).unwrap_or(0) as u8,
            holdable_items: player.stats[STAT_HOLDABLE_ITEMS],
            holdable_item: player.stats[STAT_HOLDABLE_ITEM],
            max_health: player.stats[STAT_MAX_HEALTH] as i16 as i32,
            sentry_deployed: player.raw_field(106).unwrap_or(0) != 0,
            levitation_debounce: player.raw_field(53).unwrap_or(0) as i32,
            force_rage_recovery_time: player.force_rage_recovery_time(),
            force_grip_cripple: player.raw_field(111).unwrap_or(0) != 0,
            zoom_lock_time: 0,
            force_hand_extend: player.force_hand_extend(),
            active_force_pass: player.raw_field(72).unwrap_or(0) as u8,
            force_dodge_anim: player.raw_field(89).unwrap_or(0) as u16,
            in_air_animation: player.in_air_animation(),
            legs_timer_at_entry: player.legs_timer(),
            force_hand_extend_time: 0,
            force_jump_charge: 0.0,
            force_jump_flip: false,
            force_restricted: player.force_restricted(),
            true_non_jedi: player.true_non_jedi(),
            falling_to_death: player.falling_to_death(),
            saber_lock_frame: player.saber_lock_frame(),
            vehicle_entity_num: player.vehicle_entity_num(),
            emplaced_index: player.emplaced_index(),
            zoom_mode: player.zoom_mode(),
            zoom_locked: player.zoom_locked(),
            zoom_time: player.zoom_time(),
            zoom_fov: player.zoom_fov(),
            weapon_charge_time: player.weapon_charge_time(),
            weapon_charge_subtract_time: player.raw_field(67).unwrap_or(0) as i32,
            rocket_lock_index: player.raw_field(24).unwrap_or(0) as u16,
            rocket_lock_time: f32::from_bits(player.raw_field(79).unwrap_or(0)),
            rocket_target_time: f32::from_bits(player.raw_field(71).unwrap_or(0)),
            rocket_last_valid_time: 0.0,
            footstep_time: 0,
            forced_saber: player.is_jedi_master()
                || player.is_true_jedi()
                || player.duel_in_progress(),
            weapon: player.weapon(),
            weapon_state: player.weapon_state(),
            weapon_time: player.weapon_time(),
            saber_move: player.saber_move(),
            saber_blocked: player.saber_blocked(),
            saber_blocking: 0,
            saber_attack_chain_count: player.saber_attack_chain_count(),
            saber_anim_level: player.saber_style(),
            saber_anim_level_base: 0,
            slope_recalc_time: 0,
            read_foot_bolts: false,
            force_jump_sound: false,
            saber_holstered: player.saber_holstered(),
            saber_in_flight: player.saber_in_flight(),
            saber_entity_num: player.saber_entity_num(),
            saber_lock_time: player.saber_lock_time(),
            saber_can_throw: player.raw_field(49).unwrap_or(0) != 0,
            saber_lock_enemy: player.raw_field(110).unwrap_or(0) as u16,
            saber_lock_advance: player.raw_field(120).unwrap_or(0) != 0,
            torso_anim: player.torso_animation(),
            torso_timer: player.torso_timer(),
            torso_flip: player.torso_flip(),
            legs_anim: player.leg_animation(),
            legs_timer: player.legs_timer(),
            legs_flip: player.leg_flip(),
            entity_animations: None,
            broken_limbs: player.broken_limbs(),
            hacking_time: player.raw_field(91).unwrap_or(0) as i32,
            saber_anim_speed_scales: HiltSpeedScales::default(),
            saber_special_deferred: 0,
            input_freeze_active: false,
            wall_view_locked: false,
            saber_deferred_active: false,
            duel_in_progress: player.duel_in_progress(),
            duel_time: player.duel_time(),
            ammo: std::array::from_fn(|index| player.ammo_value(index).unwrap_or(0)),
            weapons: player.owned_weapons(),
            entity_flags: player.entity_flags(),
            has_detpack_planted: player.has_detpack_planted(),
            powerup_deadlines: player.powerups.map(|value| value as i32),
            // `lastHitLoc[0..2]` are protocol-26 fields 102, 105 and 100.
            last_hit_location: [102, 105, 100]
                .map(|index| f32::from_bits(player.raw_field(index).unwrap_or(0))),
        }
    }
}

/// `pm->touchents` with `pm->numtouch` (`MAXTOUCH`): what this move stood on or walked
/// into, which the game turns into `touch` calls (`ClientImpacts`, `g_active.c:493-521`).
/// A fixed array, so a move never allocates.
#[derive(Clone, Copy, Debug)]
pub struct TouchedEntities {
    entities: [u16; Self::MAXTOUCH],
    count: u8,
}

impl Default for TouchedEntities {
    fn default() -> Self {
        Self {
            entities: [0; Self::MAXTOUCH],
            count: 0,
        }
    }
}

impl TouchedEntities {
    /// `MAXTOUCH`.
    pub const MAXTOUCH: usize = 32;

    /// What the move touched, in the order it touched them.
    pub fn entities(&self) -> &[u16] {
        &self.entities[..usize::from(self.count)]
    }

    /// `PM_AddTouchEnt` (`bg_pmove.c:896-915`): the world is never one of them, nothing
    /// is listed twice, and the thirty-third is dropped.
    fn add(&mut self, entity: u16) {
        if entity == ENTITY_NUMBER_WORLD
            || usize::from(self.count) >= Self::MAXTOUCH
            || self.entities().contains(&entity)
        {
            return;
        }
        self.entities[usize::from(self.count)] = entity;
        self.count += 1;
    }

    fn clear(&mut self) {
        self.count = 0;
    }
}

#[derive(Clone, Debug)]
pub struct Predictor {
    zoom_history: weapon_history::ZoomHistory,
    /// The box of the last move (`pm->mins`, `pm->maxs`), which a server links the player's
    /// entity with; a standing player's until a command has run.
    box_bounds: ([f32; 3], [f32; 3]),
    touched: TouchedEntities,
    /// Server-only landing request; consumed after this command, never serialized.
    vehicle_landing: Option<u16>,
    events: crate::predicted_events::PredictedEvents,
    state: MovementState,
    config: MovementConfig,
    animation_lengths: Option<crate::pmove_anim::SharedAnimationLengths>,
    /// The game type of the move under way (`pm->gametype`), from its context.
    gametype: i32,
    /// An NPC's move: its box, its class and its skeleton ([`npc::NpcBody`]); `None` for a
    /// player's.
    npc: Option<npc::NpcBody>,
    /// This slice is a server's first of a player's command, whose `pmove_t` the game has
    /// just cleared: its box before `PM_CheckDuck` is zero ([`wall_moves`]).
    cleared_box: bool,
    /// JoF EternalJK's `cg_fakeNoclip`: this client-side predictor flies in noclip while
    /// the server is told the player stands still (`pm->fakeNoclip`). Never set on a server.
    fake_noclip: bool,
    /// The water level as the slice began (`pml.previous_waterlevel`, taken right after
    /// `PM_SetWaterLevel`, `bg_pmove.c:10705-10707`), which `PM_WaterEvents` compares with.
    water_entry: u8,
    /// `gPMDoSlowFall`, decided as the slice starts ([`wall_moves::slow_fall`]).
    slow_fall: bool,
    /// A vehicle NPC's move: its vehicle ([`vehicle`]); `None` for anyone else's.
    vehicle: Option<Box<crate::vehicle::Vehicle>>,
    /// `ps.moveDir`, which a vehicle moves along (no wire field).
    move_dir: [f32; 3],
    /// The view `pml.forward` and `pml.right` are taken from (`bg_pmove.c:10641`), before a
    /// vehicle's `Update` turns it to the vehicle's orientation: its air move's axes.
    move_view: [f32; 3],
    /// A rider's move: the vehicle it rides ([`riding`]); `None` on foot.
    riding: Option<riding::Riding>,
    /// A vehicle's bumps in this move, judged as they happen and handed to the game once
    /// it is over ([`vehicle_impact`]).
    impacts: [vehicle_impact::JudgedImpact; vehicle_impact::MOST_IMPACTS],
    impact_count: u8,
    /// What a vehicle's move may bump into, as the game describes it
    /// ([`vehicle::VehicleGame::impact_bodies`]): filled for the move, empty otherwise.
    impact_bodies: Vec<vehicle_impact::ImpactBody>,
}

impl Predictor {
    pub fn from_player_state(player: &PlayerState, config: MovementConfig) -> Self {
        let mut state = MovementState::from_player_state(player);
        state.saber_anim_speed_scales = HiltSpeedScales(config.saber_anim_speed_scales);
        Self {
            state,
            events: Default::default(),
            zoom_history: Default::default(),
            box_bounds: ([-15.0, -15.0, -24.0], [15.0, 15.0, 40.0]),
            touched: TouchedEntities::default(),
            vehicle_landing: None,
            config,
            animation_lengths: None,
            gametype: 0,
            npc: None,
            cleared_box: false,
            fake_noclip: false,
            water_entry: 0,
            slow_fall: false,
            vehicle: None,
            move_dir: [0.0; 3],
            move_view: [0.0; 3],
            riding: None,
            impacts: Default::default(),
            impact_count: 0,
            impact_bodies: Vec::new(),
        }
    }

    pub fn from_state(mut state: MovementState, config: MovementConfig) -> Self {
        // Synthetic callers historically supplied only speed. Network seeds
        // use from_player_state and must preserve authoritative zero basespeed.
        if state.base_speed == 0.0 {
            state.base_speed = state.speed;
        }
        Self {
            state,
            events: Default::default(),
            zoom_history: Default::default(),
            box_bounds: ([-15.0, -15.0, -24.0], [15.0, 15.0, 40.0]),
            touched: TouchedEntities::default(),
            vehicle_landing: None,
            config,
            animation_lengths: None,
            gametype: 0,
            npc: None,
            cleared_box: false,
            fake_noclip: false,
            water_entry: 0,
            slow_fall: false,
            vehicle: None,
            move_dir: [0.0; 3],
            move_view: [0.0; 3],
            riding: None,
            impacts: Default::default(),
            impact_count: 0,
            impact_bodies: Vec::new(),
        }
    }

    /// Update server-advertised roll policy without changing predicted state.
    pub fn set_roll_rules(&mut self, rules: crate::pmove_roll::RollRules) {
        self.config.roll_rules = rules;
    }

    /// Fly in noclip from the next command on, as `cg_fakeNoclip` does
    /// (`cg_predict.c:1721-1727`): the move type is forced to `PM_NOCLIP` each command.
    pub fn set_fake_noclip(&mut self, on: bool) {
        self.fake_noclip = on;
    }

    /// Refresh movement policy without discarding the replay state.
    pub fn set_config(&mut self, config: MovementConfig) {
        self.config = config;
        self.state.saber_anim_speed_scales = HiltSpeedScales(config.saber_anim_speed_scales);
    }

    /// The sabers' `animSpeedScale` and `moveSpeedScale` (`BG_MySaber`: 1 for a hand
    /// without a saber), as [`MovementConfig`] carries them.
    pub fn set_saber_scales(&mut self, animation: [f32; 2], movement: [f32; 2]) {
        self.set_config(MovementConfig {
            saber_anim_speed_scales: animation,
            saber_speed_scales: movement,
            ..self.config
        });
    }

    /// Supply model animation timings for saber animation prediction.
    pub fn set_animation_lengths(
        &mut self,
        lengths: std::sync::Arc<dyn crate::pmove_anim::AnimationLengths>,
    ) {
        self.animation_lengths = Some(lengths);
    }

    /// Server side: the game copied this state into the player's entity, as
    /// `ClientThink_real` and `ClientEndFrame` do (`BG_PlayerStateToEntityState`).
    /// Starts the rule of [`MovementState::entity_animations`].
    pub fn copied_to_entity(&mut self) {
        self.state.entity_animations = Some([self.state.legs_anim, self.state.torso_anim]);
    }

    /// Server side: this state and the last command's events into a wire player state.
    /// `events[]` keeps the two newest by the parity of their sequence
    /// (`BG_AddPredictableEventToPlayerstate`), so `player` must be the one the
    /// previous commands were written to.
    pub fn write_player_state(&self, player: &mut PlayerState) {
        self.state.write_player_state(player);
        for event in self.events.iter() {
            let (slot, parameter) = if event.sequence & 1 == 0 {
                (27, 65)
            } else {
                (28, 60)
            };
            player.set_raw_field(slot, u32::from(event.event));
            player.set_raw_field(parameter, u32::from(event.parameter));
        }
    }

    /// A predictor with this one's rules and animation lengths that starts again from a
    /// wire state, its entity a copy of it: what a client builds for every snapshot. The
    /// server, which keeps the state the wire does not carry, keeps `saberBlocking` — the
    /// mode of the move in progress, which `WP_SaberCanBlock` reads.
    /// The movement and the events a saber lock's opponent's move reaches.
    pub(crate) fn lock_parts(
        &mut self,
    ) -> (
        &mut MovementState,
        &mut crate::predicted_events::PredictedEvents,
    ) {
        (&mut self.state, &mut self.events)
    }

    pub fn reseeded(&self, player: &PlayerState) -> Self {
        let mut fresh = Self::from_player_state(player, self.config);
        fresh.animation_lengths = self.animation_lengths.clone();
        fresh.npc = self.npc;
        if self.config.authoritative {
            fresh.state.saber_blocking = self.state.saber_blocking;
            // `ps.zoomLockTime` is no wire field either: an NPC sniper's zoom locks by it.
            fresh.state.zoom_lock_time = self.state.zoom_lock_time;
            // Nor is `ps.slopeRecalcTime`: a slope pose steps once per 100 ms by it.
            fresh.state.slope_recalc_time = self.state.slope_recalc_time;
        }
        // `ps.rocketLastValidTime`, `ps.footstepTime` and `fd.saberAnimLevelBase` are no wire fields: the
        // server's copies are the only ones.
        fresh.state.rocket_last_valid_time = self.state.rocket_last_valid_time;
        fresh.state.footstep_time = self.state.footstep_time;
        fresh.state.saber_anim_level_base = self.state.saber_anim_level_base;
        fresh.copied_to_entity();
        fresh
    }

    /// The box the last move used: what `ClientThink_real` copies to the entity's
    /// `r.mins` and `r.maxs` before it links it.
    pub fn box_bounds(&self) -> ([f32; 3], [f32; 3]) {
        self.box_bounds
    }

    /// Sets the box the move left (`pm->mins`, `pm->maxs`), as the game links it.
    pub fn set_box_bounds(&mut self, bounds: ([f32; 3], [f32; 3])) {
        self.box_bounds = bounds;
    }

    /// A server command landed on an entity which may be a boardable vehicle.
    pub fn vehicle_landing(&self) -> Option<u16> {
        self.vehicle_landing
    }

    /// What the last move stood on or walked into (`pm->touchents`), which the game
    /// hands to each one's `touch` (`ClientImpacts`).
    pub fn touched_entities(&self) -> &[u16] {
        self.touched.entities()
    }

    /// The same list by value, for a caller that cannot hold a borrow of the player
    /// while it looks through the world. It is a fixed array, so this allocates nothing.
    pub fn touched(&self) -> TouchedEntities {
        self.touched
    }

    /// `ClientEndFrame`'s `ps.stats[STAT_HEALTH] = ent->health`: the game owns a player's
    /// health, movement only reads it.
    /// `fd.saberAnimLevelBase`, which the game sets (a style cycled) and no client is sent.
    pub fn set_saber_anim_level_base(&mut self, style: u8) {
        self.state.saber_anim_level_base = style;
    }

    /// `ps.basespeed` (and `speed`), which `ClientThink_real` sets every think — to
    /// `g_speed`, or to 0 for a duellist waiting for its duel to begin.
    pub fn set_base_speed(&mut self, speed: f32) {
        self.state.base_speed = speed;
        self.state.speed = speed;
    }

    pub fn set_health(&mut self, health: i32) {
        self.state.health = health;
    }

    /// `fd.forceJumpCharge` and `ps.forceJumpFlip` as the game left them (an NPC's AI and
    /// `ForceJump`): the move reads the charge and plays the flip.
    pub fn set_force_jump(&mut self, charge: f32, flip: bool) {
        self.state.force_jump_charge = charge;
        self.state.force_jump_flip = flip;
    }

    /// `ps.forceJumpFlip` after the move: still to be played.
    pub fn pending_force_jump_flip(&self) -> bool {
        self.state.force_jump_flip
    }

    /// `ps.saberBlocking`, the blocking mode, as the game zeroes it (a knockdown).
    pub fn set_saber_blocking(&mut self, mode: u8) {
        self.state.saber_blocking = mode;
    }

    /// `ps.saberBlocked` as the game raised the saber (`WP_SaberBlockNonRandom`): the
    /// next command's `PM_WeaponLightsaber` takes it up.
    pub fn set_saber_blocked(&mut self, quadrant: u8) {
        self.state.saber_blocked = quadrant;
    }

    /// `G_SetAnim` → `BG_SetAnim` on the movement's state with its own animation table;
    /// nothing without one (the reference would set the animation with a zero length).
    /// The torso's timer and the weapon's, which the game sets outside a move (a trigger
    /// the player is pressing holds both).
    pub fn set_torso_timer(&mut self, milliseconds: i32) {
        self.state.torso_timer = milliseconds;
    }

    pub fn set_weapon_time(&mut self, milliseconds: i32) {
        self.state.weapon_time = milliseconds;
    }

    pub fn set_animation_parts(&mut self, parts: u8, animation: u16, flags: u8) {
        if let Some(lengths) = self.animation_lengths.clone() {
            crate::pmove_anim::set_animation(
                &mut self.state,
                parts,
                animation,
                flags,
                lengths.as_ref(),
            );
        }
    }

    /// `G_CheckClientIdle`'s break: an idle playing on the legs or the torso is ended
    /// (its timer 0; the torso's weapon time 0 and the saber move `ready` too). Returns
    /// whether one was.
    pub fn break_idle(&mut self, legs: bool, torso: bool, ready_move: u32) -> bool {
        if legs {
            self.state.legs_timer = 0;
        }
        if torso {
            self.state.torso_timer = 0;
            self.state.weapon_time = 0;
            self.state.saber_move = ready_move;
        }
        legs || torso
    }

    /// `ps.torsoAnim` set outright, as `G_CheckClientIdle` sets `TORSO_RAISEWEAP1`.
    pub fn set_torso_anim_raw(&mut self, animation: u16) {
        self.state.torso_anim = animation;
    }

    /// The animation table this predictor times animations with, if one was given.
    pub fn animation_lengths(&self) -> Option<&dyn crate::pmove_anim::AnimationLengths> {
        self.animation_lengths.as_deref()
    }

    /// The same table, shared.
    pub fn shared_animation_lengths(
        &self,
    ) -> Option<std::sync::Arc<dyn crate::pmove_anim::AnimationLengths>> {
        self.animation_lengths.clone()
    }

    /// The events the last command raised, oldest first.
    pub fn command_events(
        &self,
    ) -> impl Iterator<Item = crate::predicted_events::PredictedEvent> + '_ {
        self.events.iter()
    }

    /// Whether a command since the last call read the foot bolts
    /// ([`MovementState::read_foot_bolts`]); clears it.
    pub fn take_foot_bolt_read(&mut self) -> bool {
        std::mem::take(&mut self.state.read_foot_bolts)
    }

    /// Whether a Force jump began since the last call ([`MovementState::force_jump_sound`]);
    /// clears it.
    pub fn take_force_jump_sound(&mut self) -> bool {
        std::mem::take(&mut self.state.force_jump_sound)
    }

    pub fn state(&self) -> &MovementState {
        &self.state
    }

    /// The state to change in place between moves: what the vehicle code does to a
    /// rider or its vehicle (`g_vehicles.c`). The caller writes it back to the wire state.
    pub fn state_mut(&mut self) -> &mut MovementState {
        &mut self.state
    }

    /// Mirrors `codemp` `Pmove`: reject stale commands, clamp a lost second,
    /// and chop long commands at either `pmove_msec` or 66 ms.
    pub fn predict_command(&mut self, command: UserCommand, collision: &impl MovementCollision) {
        self.predict_command_in(command, collision, &MoveContext::CLIENT);
    }

    /// [`Self::predict_command`] with what the game knows of the player (see
    /// [`MoveContext`]), as a server moves it.
    pub fn predict_command_in(
        &mut self,
        command: UserCommand,
        collision: &impl MovementCollision,
        context: &MoveContext,
    ) {
        let mut outcome = crate::pmove_saber_lock::LockOutcome::default();
        self.predict(command, collision, context, None, &mut outcome);
    }

    /// [`Self::predict_command_in`] for a player in a saber lock: the lock pushed or broken
    /// changes the opponent's movement too, as the reference's does (`genemy`), and what
    /// it asks of the game comes back.
    pub fn predict_command_locked(
        &mut self,
        command: UserCommand,
        collision: &impl MovementCollision,
        context: &MoveContext,
        mut lock: crate::pmove_saber_lock::LockContext,
    ) -> crate::pmove_saber_lock::LockOutcome {
        let mut outcome = crate::pmove_saber_lock::LockOutcome::default();
        self.predict(command, collision, context, Some(&mut lock), &mut outcome);
        outcome
    }

    fn predict(
        &mut self,
        command: UserCommand,
        collision: &impl MovementCollision,
        context: &MoveContext,
        lock: Option<&mut crate::pmove_saber_lock::LockContext>,
        outcome: &mut crate::pmove_saber_lock::LockOutcome,
    ) {
        self.predict_with(command, collision, context, lock, outcome, None);
    }

    /// `Pmove` over a command, with the game's vehicle functions for a vehicle's move.
    fn predict_with<'d>(
        &mut self,
        command: UserCommand,
        collision: &impl MovementCollision,
        context: &MoveContext,
        mut lock: Option<&mut crate::pmove_saber_lock::LockContext>,
        outcome: &mut crate::pmove_saber_lock::LockOutcome,
        mut game: Option<&mut (dyn vehicle::VehicleGame + 'd)>,
    ) {
        self.gametype = context.gametype;
        self.events.clear();
        self.vehicle_landing = None;
        self.state.input_freeze_active = false;
        self.state.saber_deferred_active = false;
        // cg_predict.c:1109-1118: fixed mode updates angles even when this
        // raw command falls into an already-predicted fixed-time bucket.
        // That is the client's doing, not `Pmove`'s: a server leaves them alone.
        if self.config.fixed_millis.is_some() && !self.config.authoritative {
            self.update_view_angles(&command);
        }
        if command.server_time <= self.state.command_time {
            return;
        }
        // CG_PredictPlayerState, cg_predict.c:1239-1241: round only the
        // prediction copy, never the command serialized onto the wire.
        let final_time = self.config.command_time(command.server_time);
        if final_time < self.state.command_time {
            return;
        }
        if final_time > self.state.command_time.wrapping_add(1_000) {
            self.state.command_time = final_time.wrapping_sub(1_000);
        }
        // `Pmove` (`bg_pmove.c:11185-11191`): a player falling to death has no controls
        // of its own — it moves as whatever threw it in left it moving.
        let mut command = command;
        if self.state.falling_to_death != 0 {
            (
                command.forward_move,
                command.right_move,
                command.up_move,
                command.buttons,
            ) = (0, 0, 0, 0);
        }
        let mut repeated_command = command;
        // The trace mask is chosen once per command: a dead player's leaves bodies out.
        if self.fake_noclip && !self.config.authoritative {
            self.state.movement_type = PM_NOCLIP;
        }
        // `CG_PredictPlayerState` works the base style out before every `Pmove`
        // (`cg_predict.c:1335-1347`): it is no wire field.
        if !self.config.authoritative
            && let Some(hands) = self.config.saber_hands
        {
            hands.apply(&mut self.state);
        }
        let dead = self.state.movement_type == PM_DEAD;
        // A player the server walks through others (`GHOST_KNOWN_FLAG`: amghost, the grace
        // after unghosting inside someone, a duel's walk-apart) loses `CONTENTS_BODY` and
        // `CONTENTS_PLAYERCLIP` too (`cg_predict.c:1299-1313`); a client that kept them
        // stops dead where the server walks on, and the corrections shake the view.
        let left_out = if dead {
            Some(flight::BODY)
        } else if !self.config.authoritative
            && crate::prediction_policy::passes_through_players(self.state.force_powers_known)
        {
            Some(flight::BODY_AND_PLAYER_CLIP)
        } else {
            None
        };
        // `ClientThink_real` clears its `pmove_t` for every command (`g_active.c:2754`) and
        // gives only an NPC its entity's box (`:3007-3010`); a client's persists.
        self.cleared_box =
            self.config.authoritative && self.npc.is_none() && self.state.client_num < MAX_CLIENTS;
        while self.state.command_time != final_time {
            let maximum = self
                .config
                .fixed_millis
                .map_or(66, |value| value.clamp(8, 33));
            let millis = final_time
                .wrapping_sub(self.state.command_time)
                .min(maximum);
            let mut slice = repeated_command;
            slice.server_time = self.state.command_time.wrapping_add(millis);
            let opponent = lock
                .as_deref_mut()
                .and_then(|lock| lock.opponent(context.saber_offense));
            if let Some(left_out) = left_out {
                self.pmove_single_with(
                    slice,
                    millis,
                    &flight::WithoutBodies(collision, left_out),
                    context,
                    opponent,
                    outcome,
                    game.as_deref_mut(),
                );
            } else {
                self.pmove_single_with(
                    slice,
                    millis,
                    collision,
                    context,
                    opponent,
                    outcome,
                    game.as_deref_mut(),
                );
            }
            self.cleared_box = false;
            if self.state.movement_flags & PMF_JUMP_HELD != 0 {
                // The C wrapper changes repeated slices to a held, non-analog
                // jump value after the first `PmoveSingle`.
                repeated_command.up_move = 20;
            }
        }
        self.zoom_history.record(&self.state);
    }

    /// [`Self::pmove_single`] with the game's vehicle functions for a vehicle's move.
    #[allow(clippy::too_many_arguments)]
    fn pmove_single_with<'d>(
        &mut self,
        mut command: UserCommand,
        millis: i32,
        collision: &impl MovementCollision,
        context: &MoveContext,
        opponent: Option<crate::pmove_saber_lock::LockOpponent>,
        outcome: &mut crate::pmove_saber_lock::LockOutcome,
        mut game: Option<&mut (dyn vehicle::VehicleGame + 'd)>,
    ) {
        self.state.wall_view_locked = false;
        // `PmoveSingle` opens by cancelling an attack pressed together with a holdable
        // (`bg_pmove.c:10173-10183`), before anything reads the buttons: a spectator
        // holding both does not get alt-attack's turbo. A JA+ client leaves melee's
        // buttons alone ([`crate::pmove_japlus`]).
        const USE_HOLDABLE: u16 = 4;
        if self
            .config
            .ja_plus
            .cancels_attack_with_holdable(self.state.weapon)
        {
            for attack in [1, 128] {
                if command.buttons & attack != 0 && command.buttons & USE_HOLDABLE != 0 {
                    command.buttons &= !(attack | USE_HOLDABLE);
                }
            }
        }
        crate::pmove_emplaced::alternate_is_primary(&self.state, &mut command);
        // `gPMDoSlowFall = PM_DoSlowFall()` (`bg_pmove.c:10213`).
        self.slow_fall = wall_moves::slow_fall(&self.state);
        // `PmoveSingle`'s own `pm->numtouch = 0` (`bg_pmove.c:10219-10220`): only the
        // last slice's touches reach the game.
        self.touched.clear();
        let previous_command_time = self.state.command_time;
        self.state.command_time = command.server_time;
        crate::pmove_input_freeze::apply(
            &mut self.state,
            &mut command,
            millis,
            collision,
            self.animation_lengths.as_deref(),
            self.config.ja_plus,
        );
        // A rider facing its vehicle's hyperspace point (`bg_pmove.c:10579-10595`).
        self.face_hyperspace(&mut command, millis);
        // The view the last command left, which the wall moves read (they run before
        // `PM_UpdateViewAngles` in the reference, `bg_pmove.c:10597-10636`).
        let previous_view = self.state.view_angles;
        self.update_view_angles(&command);
        crate::pmove_roll::prepare_command(&mut self.state, &mut command, self.config.roll_rules);
        let predict_weapon = if self.riding_move() {
            // A rider's weapon (`bg_pmove.c:11081-11113`), on the server.
            self.config.authoritative
                && self.rider_weapon(&mut command)
                && crate::pmove_weapon::predicts_rider_command(
                    &self.state,
                    &command,
                    self.animation_lengths.is_some(),
                    self.config.debug_melee,
                )
        } else {
            crate::pmove_weapon::predicts_command(
                &self.state,
                &command,
                self.animation_lengths.is_some(),
                self.config.debug_melee,
            )
        };
        let cancel_zoom = if predict_weapon {
            crate::pmove_weapon_charge::prepare(
                &mut self.state,
                &mut command,
                previous_command_time,
                &mut self.events,
            )
        } else {
            false
        };
        // `PM_CmdForSaberMoves` (`bg_pmove.c:10482`), after the roll's keys.
        let saber_held_view = self.command_for_saber_moves(&mut command, previous_view);
        // `BG_AdjustClientSpeed`: "vehicles manage their own speed" (`bg_pmove.c:8350-8358`).
        if !self.vehicle_move() {
            crate::pmove_speed::adjust(&mut self.state, &command, self.config);
        }
        // `bg_pmove.c:10490-10494`: "make sure walking button is clear if they are
        // running, to avoid proxy no-footsteps cheats".
        const BUTTON_WALKING: u16 = 16;
        if command.forward_move.unsigned_abs() > 64 || command.right_move.unsigned_abs() > 64 {
            command.buttons &= !BUTTON_WALKING;
        }
        // "set the talk balloon flag", next in `PmoveSingle`.
        crate::pmove_talk::set_talk_flag(&mut self.state.entity_flags, command.buttons);
        if predict_weapon {
            crate::pmove_weapon_charge::adjust_zoom(
                &mut self.state,
                &command,
                previous_command_time,
                &mut self.events,
            );
            crate::pmove_weapon::adjust_attack_flags(&mut self.state, &command);
            crate::pmove_weapon_charge::convert_attack(&mut self.state, &mut command);
        } else if self.piloted_vehicle() || crate::pmove_emplaced::owns(&self.state) {
            // A piloted vehicle's weapon is its own (`PM_Weapon`'s vehicle branch), as is a
            // gun's, but their firing flags are `PM_AdjustAttackStates`' as anyone's
            // (`bg_pmove.c:10516`).
            crate::pmove_weapon::adjust_attack_flags(&mut self.state, &command);
        } else {
            // A weapon this port does not run still loses the respawn latch.
            crate::pmove_weapon::clear_respawned(&mut self.state, &command);
        }
        // `bg_pmove.c:10527-10534`: a player with the chat open cannot move or press
        // anything; only the talk button survives, for the later slices of a long command.
        use crate::pmove_talk::BUTTON_TALK;
        if command.buttons & BUTTON_TALK != 0 {
            command.buttons = BUTTON_TALK;
            (command.forward_move, command.right_move, command.up_move) = (0, 0, 0);
        }
        let wall_moves = self.wall_moves_live();
        let saber_holds_view = saber_view::move_holds_view(self.state.saber_move);
        if wall_moves || saber_holds_view || saber_held_view {
            // `bg_pmove.c:10597-10625`, on the last command's view: the wall moves, then the
            // saber specials' hold; then the view again (`:10639`), from what they left in
            // `delta_angles`.
            self.state.view_angles = previous_view;
            if wall_moves {
                self.adjust_for_wall_moves(&mut command, collision);
            }
            if saber_holds_view {
                crate::pmove_input_freeze::set_view_angle(&mut self.state, &command);
            }
            self.update_view_angles(&command);
        }
        self.move_view = self.state.view_angles;
        crate::pmove_roll::update_backwards_flag(&mut self.state, &command);
        // Not holding jump — unless holding on to a wall (`bg_pmove.c:10643-10646`).
        if command.up_move < 10 && self.state.movement_flags & PMF_STUCK_TO_WALL == 0 {
            self.state.movement_flags &= !PMF_JUMP_HELD;
        }
        // `bg_pmove.c:10655-10659`: a dead (or worse) player has no input.
        if self.state.movement_type >= PM_DEAD {
            command.forward_move = 0;
            command.right_move = 0;
            command.up_move = 0;
        }
        // `codemp/game/bg_pmove.c:10701-10703` (PmoveSingle) returns here for
        // PM_INTERMISSION after commandTime and PM_UpdateViewAngles have been
        // applied, but before timers, ducking, traces, or movement.
        if super::intermission::suppresses_movement(self.state.movement_type) {
            return;
        }
        if self.non_walking_move(&command, millis, collision) {
            return;
        }
        let previous_origin = self.state.origin;
        let previous_velocity = self.state.velocity;
        let was_grounded = self.state.ground_entity_number != ENTITY_NUMBER_NONE;
        let seconds = frame_seconds(millis);
        // A vehicle's `m_fTimeModifier`: the frame time times 60 (`bg_pmove.c:10569-10578`).
        if self.vehicle_move()
            && let Some(vehicle) = self.vehicle.as_deref_mut()
        {
            vehicle.time_modifier = seconds * 60.0;
        }
        let hovering = self.hovering();
        self.sample_water(collision);
        self.water_entry = self.state.water_level;
        let bounds = self.check_duck(&command, collision);
        // `BG_VehicleAdjustBBoxForOrientation` (`bg_pmove.c:10814-10818`).
        let bounds = self.vehicle_box(bounds, collision);
        self.box_bounds = (bounds.minimums, bounds.maximums);
        // A wall run falls at half gravity (`bg_pmove.c:10726-10730`), until the slice ends.
        let gravity = self.state.gravity;
        let jetpacking = self.state.movement_type == 1;
        if jetpacking {
            self.jetpack_move(&command, bounds, collision);
        } else if self.slow_fall {
            self.state.gravity = wall_moves::halved_gravity(gravity);
        }
        self.state.legs_timer_at_entry = self.state.legs_timer;
        let mut ground = self.ground_trace(&command, bounds, collision);
        if !was_grounded && ground.walking {
            self.landing_events(
                &command,
                bounds,
                &ground,
                collision,
                previous_origin,
                previous_velocity,
            );
            if self.config.authoritative {
                self.vehicle_landing = crate::vehicle_auto_board::landing_candidate(
                    &self.state,
                    self.state.ground_entity_number,
                );
            }
            self.start_landing_timer(previous_velocity[2]);
        }
        if hovering {
            self.hover_trace(
                &command,
                bounds,
                &mut ground,
                collision,
                seconds,
                game.as_deref_mut(),
            );
        }
        // PM_GroundTrace precedes PM_DropTimers (bg_pmove.c:10821,10845).
        self.drop_timers(millis);
        crate::force_powers::select(&mut self.state, &command);
        if ground.walking {
            self.state.force_jump_start_height = 0.0;
        }
        // "vehicles don't use deadmove", but for the animals (`bg_pmove.c:10832-10843`).
        if self.state.movement_type == PM_DEAD
            && !self.vehicle.as_ref().is_some_and(|vehicle| {
                self.vehicle_move() && vehicle.kind() != crate::vehicle_fields::kind::ANIMAL
            })
        {
            self.dead_move(&ground);
        }
        self.bounce_off_npc(&mut command, context);
        self.vehicle_think(&command, collision, game.as_deref_mut());
        if self.riding_move() || self.npc_aboard() {
            // "don't even run physics on a player if he's on a vehicle - he goes where the
            // vehicle goes" (`bg_pmove.c:11028-11031`); nor on an NPC that is no vehicle with
            // an `m_iVehicleNum` (a droid unit, even one let go: `ENTITYNUM_NONE`).
        } else if self.state.movement_type == 2 || self.flying_normal() {
            // PM_FLOAT or FLY_NORMAL dispatch, bg_pmove.c:11034-11038.
            self.fly_move(&command, seconds, bounds, &ground, collision);
        } else if self.flying_vehicle() {
            self.fly_vehicle_move(seconds, bounds, &ground, collision);
        } else if self.state.movement_flags & PMF_TIME_WATERJUMP != 0 {
            self.water_jump_move(seconds, bounds, &ground, collision);
        } else if let Some(hook) = self.hook_move(&command) {
            // JA+'s hook (see [`grapple`]): a pull or a hang on the rope, both carried
            // by an air move, whatever the water or ground below.
            if hook == grapple::HookMove::Pull {
                self.grapple_pull(&mut ground);
            }
            self.air_move(
                &mut command,
                seconds,
                bounds,
                &mut ground,
                collision,
                context,
            );
            if hook == grapple::HookMove::Hang {
                self.rope_hang(previous_origin, seconds);
            }
        } else if self.state.water_level > 1 {
            self.water_move(&command, seconds, bounds, &ground, collision);
        } else if ground.walking {
            self.walk_move(
                &mut command,
                seconds,
                bounds,
                &mut ground,
                collision,
                context,
            );
        } else {
            self.air_move(
                &mut command,
                seconds,
                bounds,
                &mut ground,
                collision,
                context,
            );
        }
        self.vehicle_impacts(&command, game.as_deref_mut());
        let airborne = self.state.ground_entity_number == ENTITY_NUMBER_NONE;
        let mut landed = self.ground_trace(&command, bounds, collision);
        if airborne && landed.walking {
            self.landing_events(
                &command,
                bounds,
                &landed,
                collision,
                previous_origin,
                previous_velocity,
            );
            if self.config.authoritative {
                self.vehicle_landing = crate::vehicle_auto_board::landing_candidate(
                    &self.state,
                    self.state.ground_entity_number,
                );
            }
            self.start_landing_timer(previous_velocity[2]);
        }
        if hovering {
            self.hover_trace(
                &command,
                bounds,
                &mut landed,
                collision,
                seconds,
                game.as_deref_mut(),
            );
        }
        self.sample_water(collision);
        if self.npc_without_weapon(&command) {
            // `PM_Weapon` returned at its first lines (`bg_pmove.c:6668-6682`).
        } else if self.piloted_vehicle() {
            self.piloted_weapon(&command, millis, game.as_deref_mut());
        } else if crate::pmove_emplaced::owns(&self.state) {
            crate::pmove_emplaced::weapon(
                &mut self.state,
                &mut command,
                millis,
                self.animation_lengths.as_deref(),
                &mut self.events,
            );
        } else if predict_weapon {
            let buttons = crate::pmove_weapon::advance_events_in(
                &mut self.state,
                &command,
                millis,
                self.animation_lengths.as_deref(),
                cancel_zoom,
                &mut self.events,
                Some(collision),
                context,
                (bounds.minimums, bounds.maximums),
                self.config.legacy_fixes,
                self.config.debug_melee,
                self.config.ja_plus,
                opponent,
                outcome,
            );
            // A rider's pose reads the command as its saber left it (`PM_VehicleWeaponAnimate`
            // after `PM_Weapon`, one `pm->cmd`).
            if self.riding_move() {
                command.buttons = buttons;
            }
        }
        let bob_rate = events::bob_rate(&self.state, &command);
        // No footsteps for a vehicle or its rider (`bg_pmove.c:11117-11124`).
        if self.vehicle_move() || self.riding_move() {
        } else if let Some(lengths) = self.animation_lengths.as_deref() {
            if crate::pmove_roll::roll_from_crouch(
                &mut self.state,
                &command,
                collision,
                lengths,
                self.config.roll_rules,
            ) {
                self.add_event(events::EV_ROLL, 0); // bg_pmove.c:5356.
            } else {
                let slope = crate::pmove_locomotion::Slope {
                    minimum_z: bounds.minimums[2],
                    feet: context.foot_bolts,
                    collision,
                };
                crate::pmove_locomotion::footsteps(
                    &mut self.state,
                    &command,
                    self.config.legacy_fixes,
                    lengths,
                    self.npc.as_ref(),
                    &slope,
                );
            }
        }
        self.advance_bob(bob_rate, millis, command.server_time, bounds, collision);
        self.water_events();
        if self.config.snap_velocity {
            self.state.velocity = self.state.velocity.map(f32::round_ties_even);
        }
        if jetpacking || self.slow_fall {
            self.state.gravity = gravity;
        }
        // `AttachRiders` (`bg_pmove.c:11138-11154`).
        if self.vehicle_move()
            && let (Some(vehicle), Some(game)) = (self.vehicle.as_deref_mut(), game)
        {
            game.attach_riders(vehicle, &self.state);
        }
        // A rider's pose (`bg_pmove.c:11157-11161`).
        self.vehicle_weapon_animate(&command);
    }

    /// Mirrors `PM_UpdateViewAngles`; movement only consumes yaw but preserving
    /// all axes makes the compatibility state reusable by the live client.
    fn update_view_angles(&mut self, command: &UserCommand) {
        // "no view changes at all" for the dead (`bg_pmove.c:7835-7837`): a corpse keeps the
        // view it died with — and an NPC's body the tilt its slope gave it.
        if self.state.movement_type != crate::PM_SPECTATOR && self.state.health <= 0 {
            return;
        }
        for axis in 0..3 {
            let mut value = command.angles[axis].wrapping_add(self.state.delta_angles[axis]) as i16;
            // `bg_pmove.c` PM_UpdateViewAngles clamps pitch to +-16000 short units (87.89
            // degrees) and writes the clamp back into delta_angles. Omitting the writeback
            // left the client's delta_angles disagreeing with the server's: the server
            // clamped and rewrote every snapshot, the client did not, so each snapshot
            // looked like a fresh authoritative view change and the local pitch escalated
            // past vertical. An owner session reached -121 degrees.
            // Stock returns from PM_UpdateViewAngles outright during intermission ("no view
            // changes at all", bg_pmove.c:7831-7833), so the clamp must not reach it; SJK
            // keeps updating the view there and that behaviour is unchanged.
            if axis == PITCH && self.state.movement_type != crate::PM_INTERMISSION {
                if value > PITCH_CLAMP {
                    self.state.delta_angles[axis] =
                        i32::from(PITCH_CLAMP).wrapping_sub(command.angles[axis]);
                    value = PITCH_CLAMP;
                } else if value < -PITCH_CLAMP {
                    self.state.delta_angles[axis] =
                        i32::from(-PITCH_CLAMP).wrapping_sub(command.angles[axis]);
                    value = -PITCH_CLAMP;
                }
            }
            self.state.view_angles[axis] = f32::from(value) * (360.0 / 65_536.0);
        }
    }

    /// `PM_DropTimers` (`bg_pmove.c:7767-7795`): the move's timer, then the legs' and
    /// the torso's animation timers — after the first ground trace, whose leg animations
    /// still see this command's timers (a choked player falling keeps its legs).
    fn drop_timers(&mut self, millis: i32) {
        if self.state.movement_time > 0 {
            if millis >= self.state.movement_time {
                self.state.movement_flags &= !PMF_ALL_TIMES;
                self.state.movement_time = 0;
            } else {
                self.state.movement_time -= millis;
            }
        }
        if self.state.legs_timer > 0 {
            self.state.legs_timer = (self.state.legs_timer - millis).max(0);
        }
        if self.state.torso_timer > 0 {
            self.state.torso_timer = (self.state.torso_timer - millis).max(0);
        }
    }

    /// PM_GroundTrace's new-contact timer, bg_pmove.c:4232-4239.
    /// Call even when crash-land sound/damage was suppressed by water or a surface.
    fn start_landing_timer(&mut self, previous_vertical_velocity: f32) {
        if previous_vertical_velocity < -200.0 {
            self.state.movement_flags |= PMF_TIME_LAND;
            self.state.movement_time = 250;
        }
    }

    /// Mirrors ordinary-player `PM_CheckDuck`: -15..15 horizontally, -24 at
    /// the feet, server-provided standing/crouching tops, and obstruction test.
    fn check_duck(&mut self, command: &UserCommand, collision: &impl MovementCollision) -> Bounds {
        if let Some(bounds) = self.riding_check_duck(collision) {
            return bounds;
        }
        if let Some(npc) = self.npc {
            return self.npc_check_duck(npc, command, collision);
        }
        let minimums = [-15.0, -15.0, -24.0];
        if self.state.movement_type == PM_DEAD {
            // `bg_pmove.c:4484-4489`: a corpse is a low box with a dead view.
            self.state.view_height = DEAD_VIEW_HEIGHT;
            return Bounds {
                minimums,
                maximums: [15.0, 15.0, DEAD_TOP],
            };
        }
        if let Some(maximum_z) = crate::pmove_roll::bounds_height(
            &mut self.state,
            collision,
            minimums,
            PLAYER_CONTENT_MASK,
        ) {
            return Bounds {
                minimums,
                maximums: [15.0, 15.0, maximum_z],
            };
        }
        // PM_CheckDuck, bg_pmove.c:4503-4509. Active rolls take precedence above.
        if command.up_move < 0 || matches!(self.state.force_hand_extend, 8 | 13 | 14) {
            self.state.movement_flags |= PMF_DUCKED;
        } else if self.state.movement_flags & PMF_DUCKED != 0 {
            if crate::pmove_posture::can_stand(
                &self.state,
                collision,
                minimums,
                PLAYER_CONTENT_MASK,
            ) {
                self.state.movement_flags &= !PMF_DUCKED;
            }
        }
        let ducked = self.state.movement_flags & PMF_DUCKED != 0;
        self.state.view_height = if ducked {
            CROUCH_VIEW_HEIGHT
        } else {
            STANDING_VIEW_HEIGHT
        };
        Bounds {
            minimums,
            maximums: [
                15.0,
                15.0,
                if ducked {
                    self.state.crouching_height
                } else {
                    self.state.standing_height
                },
            ],
        }
    }

    /// Mirrors `PM_DeadMove` (`bg_pmove.c:3462-3479`): a corpse on the ground
    /// bleeds 20 units of speed per command and never accelerates.
    fn dead_move(&mut self, ground: &GroundState) {
        if !ground.walking {
            return;
        }
        let velocity = Vec3::from_array(self.state.velocity);
        let speed = velocity.length() - 20.0;
        self.state.velocity = if speed <= 0.0 {
            [0.0; 3]
        } else {
            (velocity.normalize() * speed).to_array()
        };
    }

    /// Mirrors `PM_GroundTrace`: a 0.25-unit downward sweep, upward kick-off,
    /// and the 0.7 walkable-normal threshold.
    fn ground_trace(
        &mut self,
        command: &UserCommand,
        bounds: Bounds,
        collision: &impl MovementCollision,
    ) -> GroundState {
        let mut end = self.state.origin;
        end[2] -= 0.25;
        let trace = collision.trace(
            self.state.origin,
            bounds.minimums,
            bounds.maximums,
            end,
            PLAYER_CONTENT_MASK,
        );
        // PM_FLOAT always takes PM_GroundTraceMissed, even above a floor
        // (bg_pmove.c:4137-4143); it must not acquire walking friction/land timers.
        if trace.all_solid || trace.fraction == 1.0 || matches!(self.state.movement_type, 1 | 2) {
            if !trace.all_solid
                && let Some(lengths) = self.animation_lengths.as_deref()
            {
                // `PM_GroundTraceMissed`: the legs react to leaving the ground.
                crate::pmove_locomotion::left_the_ground(
                    &mut self.state,
                    command,
                    lengths,
                    |state| {
                        let mut below = state.origin;
                        below[2] -= 64.0;
                        collision
                            .trace(
                                state.origin,
                                bounds.minimums,
                                bounds.maximums,
                                below,
                                PLAYER_CONTENT_MASK,
                            )
                            .fraction
                            == 1.0
                            || state.movement_type == 2
                    },
                );
            }
            self.state.ground_entity_number = ENTITY_NUMBER_NONE;
            return GroundState::air(trace);
        }
        let normal = Vec3::from_array(trace.plane_normal);
        let velocity = Vec3::from_array(self.state.velocity);
        if velocity.z > 0.0 && velocity.dot(normal) > 10.0 {
            // Thrown off the ground (`bg_pmove.c:4153-4170`): into the jump animation.
            crate::pmove_locomotion::kicked_off(&mut self.state, command);
            self.state.ground_entity_number = ENTITY_NUMBER_NONE;
            return GroundState::air(trace);
        }
        if trace.plane_normal[2] < self.min_walk_normal() {
            self.state.ground_entity_number = ENTITY_NUMBER_NONE;
            return GroundState {
                trace,
                ground_plane: true,
                walking: false,
            };
        }
        self.state.ground_entity_number = trace.entity_number;
        // `PM_AddTouchEnt(trace.entityNum)` (`bg_pmove.c:4244`): what a player stands on
        // is touched, which is how a lift knows somebody is riding it.
        self.touched.add(trace.entity_number);
        // PM_GroundTrace, bg_pmove.c:4186-4191: landing ends the ledge launch.
        if self.state.movement_flags & PMF_TIME_WATERJUMP != 0 {
            self.state.movement_flags &= !(PMF_TIME_WATERJUMP | PMF_TIME_LAND);
            self.state.movement_time = 0;
        }
        GroundState {
            trace,
            ground_plane: true,
            walking: true,
        }
    }

    /// Mirrors `PM_Accelerate`: add only the missing component along wishdir,
    /// capped by `accel * frametime * wishspeed`.
    fn accelerate(
        &mut self,
        wish_direction: Vec3,
        wish_speed: f32,
        acceleration: f32,
        seconds: f32,
    ) {
        let mut velocity = Vec3::from_array(self.state.velocity);
        let add_speed = wish_speed - velocity.dot(wish_direction);
        let npc = self.state.client_num >= MAX_CLIENTS;
        let Some(acceleration_speed) =
            npc::acceleration_speed(npc, add_speed, acceleration, seconds, wish_speed)
        else {
            return;
        };
        velocity += wish_direction * acceleration_speed;
        self.state.velocity = velocity.to_array();
    }

    /// Mirrors `PM_CmdScale`; upmove is deliberately excluded in JKA so jump
    /// does not reduce horizontal running speed.
    fn command_scale(&self, command: &UserCommand) -> f32 {
        let forward = i32::from(command.forward_move);
        let right = i32::from(command.right_move);
        let maximum = forward.abs().max(right.abs());
        if maximum == 0 {
            return 0.0;
        }
        let total = ((forward * forward + right * right) as f32).sqrt();
        // `PM_CmdScale` (`bg_pmove.c:1188`) divides by `127.0 * total`, a double, and
        // rounds to float once. Dividing in single precision rounds twice and is one
        // ulp off for mixed forward/strafe input, which shows in the position's bits.
        (f64::from(self.state.speed * maximum as f32) / (127.0 * f64::from(total))) as f32
    }

    /// Mirrors normal-player `PM_WalkMove`: jump check, friction, command
    /// scaling, slope-projected basis, crouch cap, acceleration and speed-
    /// preserving ground-plane clipping before `PM_StepSlideMove`.
    fn walk_move(
        &mut self,
        command: &mut UserCommand,
        seconds: f32,
        bounds: Bounds,
        ground: &mut GroundState,
        collision: &impl MovementCollision,
        context: &MoveContext,
    ) {
        if self.check_jump(command, bounds, ground, collision, context) {
            self.air_move(command, seconds, bounds, ground, collision, context);
            return;
        }
        self.friction(seconds, ground);
        crate::pmove_dir::set_movement_direction(&mut self.state, command);
        // A vehicle's `Update` turned its view since the axes were taken.
        let view = if self.vehicle_move() {
            self.move_view
        } else {
            self.state.view_angles
        };
        let (forward, right) = flattened_axes(view);
        let normal = Vec3::from_array(ground.trace.plane_normal);
        let forward = vector_normalize(self.clip_velocity(forward, normal, OVERCLIP));
        let right = vector_normalize(self.clip_velocity(right, normal, OVERCLIP));
        // A vehicle goes along its `moveDir` at its own speed (`bg_pmove.c:3330-3375`).
        let (wish_direction, mut wish_speed) = self.vehicle_walk_wish().unwrap_or_else(|| {
            let wish_velocity =
                forward * f32::from(command.forward_move) + right * f32::from(command.right_move);
            (
                vector_normalize(wish_velocity),
                wish_velocity.length() * self.command_scale(command),
            )
        });
        if self.state.movement_flags & PMF_DUCKED != 0 {
            wish_speed = wish_speed.min(self.state.speed * DUCK_SCALE);
        }
        // PM_WalkMove :3391-3399; level >= 2 is dispatched to swimming first.
        if self.state.water_level != 0 {
            let scale = 1.0 - 0.5 * (f32::from(self.state.water_level) / 3.0);
            wish_speed = wish_speed.min(self.state.speed * scale);
        }
        let slick = ground.trace.surface_flags & SURF_SLICK != 0;
        let knockback = self.state.movement_flags & PMF_TIME_KNOCKBACK != 0;
        self.accelerate(
            wish_direction,
            wish_speed,
            if self.hovering() {
                vehicle::VEHICLE_ACCELERATION
            } else if slick || knockback {
                AIR_ACCELERATION
            } else {
                GROUND_ACCELERATION
            },
            seconds,
        );
        if slick || knockback {
            self.state.velocity[2] -= self.state.gravity * seconds;
        }
        let old_speed = Vec3::from_array(self.state.velocity).length();
        // `VectorNormalize` leaves a zero vector as it is, its signed zeros included: a
        // player brought to rest while moving towards negative x stops at -0.
        let clipped = vector_normalize(self.clip_velocity(
            Vec3::from_array(self.state.velocity),
            Vec3::from_array(ground.trace.plane_normal),
            OVERCLIP,
        )) * old_speed;
        self.state.velocity = clipped.to_array();
        if self.state.velocity[0] == 0.0 && self.state.velocity[1] == 0.0 {
            return;
        }
        self.step_slide_move(false, seconds, bounds, ground, collision);
    }

    /// Mirrors `PM_AirMove`: `PM_CheckJump` (the Force jump's rise, the wall moves), flat
    /// yaw input — none in a wall run's slow fall — air acceleration, steep-ground clipping,
    /// then gravity step-slide movement; neither for a player holding on to a wall.
    fn air_move(
        &mut self,
        command: &mut UserCommand,
        seconds: f32,
        bounds: Bounds,
        ground: &mut GroundState,
        collision: &impl MovementCollision,
        context: &MoveContext,
    ) {
        self.check_jump(command, bounds, ground, collision, context);
        self.friction(seconds, ground);
        crate::pmove_dir::set_movement_direction(&mut self.state, command);
        // A vehicle's `Update` turned its view since the axes were taken.
        let view = if self.vehicle_move() {
            self.move_view
        } else {
            self.state.view_angles
        };
        let (forward, right) = flattened_axes(view);
        let (forward, right) = (vector_normalize(forward), vector_normalize(right));
        // A hovering vehicle has air control along its `moveDir` (`bg_pmove.c:3087-3095`).
        let (wish_direction, wish_speed) = self.vehicle_air_wish().unwrap_or_else(|| {
            let mut wish_velocity = if self.slow_fall {
                Vec3::ZERO
            } else {
                forward * f32::from(command.forward_move) + right * f32::from(command.right_move)
            };
            if self.state.movement_type == 1 && !self.slow_fall {
                wish_velocity *= if command.up_move <= 0 { 0.8 } else { 2.0 };
            }
            (
                vector_normalize(wish_velocity),
                wish_velocity.length() * self.command_scale(command),
            )
        });
        let acceleration = self
            .vehicle_air_acceleration(ground)
            .unwrap_or(AIR_ACCELERATION);
        self.accelerate(wish_direction, wish_speed, acceleration, seconds);
        let stuck = self.state.movement_flags & PMF_STUCK_TO_WALL != 0;
        if ground.ground_plane
            && !stuck
            && wall_moves::ground_slide_okay(
                self.state.legs_anim,
                self.state.velocity[2],
                ground.trace.plane_normal[2],
            )
        {
            self.state.velocity = self
                .clip_velocity(
                    Vec3::from_array(self.state.velocity),
                    Vec3::from_array(ground.trace.plane_normal),
                    OVERCLIP,
                )
                .to_array();
        }
        // No gravity holding on to a wall (`bg_pmove.c:3258-3265`).
        self.step_slide_move(!stuck, seconds, bounds, ground, collision);
    }

    /// `PM_ClipVelocity` (`bg_pmove.c:925-960`): slide `velocity` along a plane. A player
    /// grabbing a wall does not slide at all; and with `g_stepSlideFix` a player on the
    /// ground is never pushed up (or down) by a plane too steep to walk on — a wall's
    /// sloped foot, an overhang — but keeps its vertical speed.
    pub(crate) fn clip_velocity(&self, velocity: Vec3, normal: Vec3, overbounce: f32) -> Vec3 {
        if self.state.movement_flags & PMF_STUCK_TO_WALL != 0 {
            return velocity;
        }
        let dot = velocity.dot(normal);
        let mut clipped = velocity
            - normal
                * if dot < 0.0 {
                    dot * overbounce
                } else {
                    dot / overbounce
                };
        if self.config.step_slide_fix
            && self.state.client_num < MAX_CLIENTS
            && self.state.ground_entity_number != ENTITY_NUMBER_NONE
            && normal.z < MIN_WALK_NORMAL
        {
            clipped.z = velocity.z;
        }
        clipped
    }

    /// Mirrors `PM_SlideMove`: half-step gravity, four collision bumps, five
    /// fixed clip planes, crease handling and timer velocity preservation.
    fn slide_move(
        &mut self,
        gravity: bool,
        seconds: f32,
        bounds: Bounds,
        ground: &GroundState,
        collision: &impl MovementCollision,
    ) -> bool {
        let mut velocity = Vec3::from_array(self.state.velocity);
        let mut primal_velocity = velocity;
        let mut end_velocity = velocity;
        if gravity {
            end_velocity.z -= self.state.gravity * seconds;
            velocity.z = (velocity.z + end_velocity.z) * 0.5;
            primal_velocity.z = end_velocity.z;
            if ground.ground_plane
                && wall_moves::ground_slide_okay(
                    self.state.legs_anim,
                    velocity.z,
                    ground.trace.plane_normal[2],
                )
            {
                velocity = self.clip_velocity(
                    velocity,
                    Vec3::from_array(ground.trace.plane_normal),
                    OVERCLIP,
                );
            }
        }
        let mut planes = [Vec3::ZERO; 5];
        let mut plane_count = 0;
        if ground.ground_plane {
            planes[0] = Vec3::from_array(ground.trace.plane_normal);
            // A wall runner never turns against a plane it may not slide up.
            if !wall_moves::ground_slide_okay(self.state.legs_anim, velocity.z, planes[0].z) {
                planes[0].z = 0.0;
                planes[0] = vector_normalize(planes[0]);
            }
            plane_count = 1;
        }
        planes[plane_count] = velocity.normalize_or_zero();
        plane_count += 1;
        let mut time_left = seconds;
        let mut bumped = false;
        for _ in 0..4 {
            let end = Vec3::from_array(self.state.origin) + velocity * time_left;
            let trace = collision.trace(
                self.state.origin,
                bounds.minimums,
                bounds.maximums,
                end.to_array(),
                PLAYER_CONTENT_MASK,
            );
            if trace.all_solid {
                velocity.z = 0.0;
                self.state.velocity = velocity.to_array();
                return true;
            }
            if trace.fraction > 0.0 {
                self.state.origin = trace.end_position;
            }
            if trace.fraction == 1.0 {
                break;
            }
            // `PM_AddTouchEnt(trace.entityNum)` (`bg_slidemove.c:728`): what the move
            // walked into; a vehicle's `PM_VehicleImpact` (`bg_slidemove.c:731-738`).
            self.touched.add(trace.entity_number);
            self.record_vehicle_impact(&trace, &mut velocity, seconds);
            bumped = true;
            time_left -= time_left * trace.fraction;
            if plane_count >= planes.len() {
                self.state.velocity = [0.0; 3];
                return true;
            }
            let mut normal = Vec3::from_array(trace.plane_normal);
            if !wall_moves::ground_slide_okay(self.state.legs_anim, velocity.z, normal.z) {
                // Wall-running: never pushed up off a sloped wall (`bg_slidemove.c:760-765`).
                normal.z = 0.0;
                normal = vector_normalize(normal);
            }
            // No nudge along the same plane holding on to a wall (`bg_slidemove.c:771-781`).
            if self.state.movement_flags & PMF_STUCK_TO_WALL == 0
                && planes[..plane_count].contains(&normal)
            {
                velocity += normal;
                continue;
            }
            planes[plane_count] = normal;
            plane_count += 1;
            let original_velocity = velocity;
            let original_end_velocity = end_velocity;
            'first_plane: for first in 0..plane_count {
                if velocity.dot(planes[first]) >= 0.1 {
                    continue;
                }
                let mut clipped = self.clip_velocity(velocity, planes[first], OVERCLIP);
                let mut end_clipped = self.clip_velocity(end_velocity, planes[first], OVERCLIP);
                for second in 0..plane_count {
                    if second == first || clipped.dot(planes[second]) >= 0.1 {
                        continue;
                    }
                    clipped = self.clip_velocity(clipped, planes[second], OVERCLIP);
                    end_clipped = self.clip_velocity(end_clipped, planes[second], OVERCLIP);
                    if clipped.dot(planes[first]) >= 0.0 {
                        continue;
                    }
                    let crease = vector_normalize(planes[first].cross(planes[second]));
                    clipped = crease * crease.dot(original_velocity);
                    end_clipped = crease * crease.dot(original_end_velocity);
                    for (third, plane) in planes[..plane_count].iter().enumerate() {
                        if third != first && third != second && clipped.dot(*plane) < 0.1 {
                            self.state.velocity = [0.0; 3];
                            return true;
                        }
                    }
                }
                velocity = clipped;
                end_velocity = end_clipped;
                break 'first_plane;
            }
        }
        if gravity {
            velocity = end_velocity;
        }
        if self.state.movement_time != 0 {
            velocity = primal_velocity;
        }
        self.state.velocity = velocity.to_array();
        bumped
    }

    /// Mirrors `PM_StepSlideMove`: retain the first slide result, try an
    /// 18-unit step from the original state, trace down, and apply the server's
    /// `g_stepSlideFix` steep-slope rejection/velocity clipping.
    fn step_slide_move(
        &mut self,
        gravity: bool,
        seconds: f32,
        bounds: Bounds,
        ground: &GroundState,
        collision: &impl MovementCollision,
    ) {
        let start_origin = self.state.origin;
        let start_velocity = self.state.velocity;
        // Holding a wall has no gravity (`bg_slidemove.c:892-895`).
        let gravity = gravity && !wall_moves::rebound_hold(self.state.legs_anim);
        if !self.slide_move(gravity, seconds, bounds, ground, collision) {
            return;
        }
        // A hovering vehicle never steps up (`bg_slidemove.c:901-909`).
        if self.vehicle_move()
            && self
                .vehicle
                .as_ref()
                .is_some_and(|vehicle| vehicle.info.hover_height > 0.0)
        {
            return;
        }
        let mut down = start_origin;
        down[2] -= STEP_SIZE;
        let down_from_start = collision.trace(
            start_origin,
            bounds.minimums,
            bounds.maximums,
            down,
            PLAYER_CONTENT_MASK,
        );
        if self.state.velocity[2] > 0.0
            && (down_from_start.fraction == 1.0
                || down_from_start.plane_normal[2] < MIN_WALK_NORMAL)
        {
            return;
        }
        let down_origin = self.state.origin;
        let down_velocity = self.state.velocity;
        let mut up = start_origin;
        let (step_height, giant) = self.step_height();
        up[2] += step_height;
        let up_trace = collision.trace(
            start_origin,
            bounds.minimums,
            bounds.maximums,
            up,
            PLAYER_CONTENT_MASK,
        );
        if up_trace.all_solid {
            return;
        }
        let step_size = up_trace.end_position[2] - start_origin[2];
        self.state.origin = up_trace.end_position;
        self.state.velocity = start_velocity;
        self.slide_move(gravity, seconds, bounds, ground, collision);
        let mut step_down = self.state.origin;
        step_down[2] -= step_size;
        let trace = collision.trace(
            self.state.origin,
            bounds.minimums,
            bounds.maximums,
            step_down,
            PLAYER_CONTENT_MASK,
        );
        // Only a player is refused a step onto a slope too steep to walk.
        let reject_steep = self.config.step_slide_fix
            && self.state.client_num < MAX_CLIENTS
            && trace.plane_normal[2] < MIN_WALK_NORMAL
            && {
                let direction = (Vec3::from_array(trace.end_position)
                    - Vec3::from_array(down_origin))
                .normalize_or_zero();
                direction.z > 1.0 - MIN_WALK_NORMAL
            };
        let accepted = !(trace.all_solid || reject_steep);
        // A rancor does not step onto a player (`bg_slidemove.c:988-1003`).
        let onto_player = accepted
            && giant
            && trace.entity_number < MAX_CLIENTS
            && self.npc.is_some_and(|npc| npc.class == npc::CLASS_RANCOR);
        let stepped = accepted && !onto_player;
        if stepped {
            self.state.origin = trace.end_position;
        } else if self.config.step_slide_fix {
            // The step is refused: the slide move's result stands.
            self.state.origin = down_origin;
            self.state.velocity = down_velocity;
        } else if onto_player {
            self.state.origin = start_origin;
            self.state.velocity = start_velocity;
        }
        // With `g_stepSlideFix` only a step taken is clipped; without it, whatever the
        // push down hit (`bg_slidemove.c:1027-1052`).
        if trace.fraction < 1.0 && (stepped || !self.config.step_slide_fix) {
            self.state.velocity = self
                .clip_velocity(
                    Vec3::from_array(self.state.velocity),
                    Vec3::from_array(trace.plane_normal),
                    OVERCLIP,
                )
                .to_array();
        }
        // The step event is raised from wherever the player ended up, a refused step
        // included: a slide up a ramp of more than two units in one command is a "step".
        if let Some(event) = events::step_event(self.state.origin[2] - start_origin[2]) {
            self.add_event(event, 0);
        }
    }
}

#[path = "pmove_events.rs"]
mod events;
#[path = "pmove_grapple.rs"]
pub mod grapple;
#[path = "pmove_water.rs"]
mod water;
#[path = "pmove_weapon_history.rs"]
mod weapon_history;

#[path = "pmove_flight.rs"]
pub mod flight;
#[path = "pmove_jetpack.rs"]
mod jetpack;

#[path = "pmove_npc.rs"]
pub mod npc;
#[path = "pmove_riding.rs"]
pub mod riding;
#[path = "pmove_vehicle.rs"]
pub mod vehicle;
#[path = "pmove_vehicle_impact.rs"]
pub mod vehicle_impact;

#[path = "pmove_force_jump.rs"]
mod force_jump;

#[path = "pmove_jump.rs"]
mod jump;

#[path = "pmove_wall_moves.rs"]
mod wall_moves;
pub use wall_moves::{in_wall_rebound, wall_hold_yaw};

#[path = "pmove_saber_base.rs"]
pub mod saber_base;
#[path = "pmove_saber_view.rs"]
mod saber_view;

#[path = "pmove_triggers.rs"]
mod triggers;

#[derive(Clone, Copy)]
struct Bounds {
    minimums: [f32; 3],
    maximums: [f32; 3],
}

#[derive(Clone, Copy)]
struct GroundState {
    trace: MovementTrace,
    ground_plane: bool,
    walking: bool,
}

impl GroundState {
    fn air(trace: MovementTrace) -> Self {
        Self {
            trace,
            ground_plane: false,
            walking: false,
        }
    }
}

/// The view's forward and right axes with the vertical part removed, not yet
/// normalised (`PM_WalkMove`, `bg_pmove.c:3318-3326`; `PM_AirMove`, `:3082-3085`).
/// The reference takes them from the full `AngleVectors`, pitch and roll included, so
/// the horizontal parts carry the pitch's cosine until the caller normalises them;
/// axes built from the yaw alone differ in the last bits.
fn flattened_axes(view_angles: [f32; 3]) -> (Vec3, Vec3) {
    let (forward, right) = flight::flight_axes(view_angles);
    (forward.with_z(0.0), right.with_z(0.0))
}

/// Mirrors `PM_ClipVelocity`, including its asymmetric 1.001 overbounce.
/// `VectorNormalize` (`q_math.c`): a zero-length vector is returned as it is, signed
/// zeros included. Two parallel planes make such a crease, and the reference carries
/// its negative zero into the velocity (`bg_slidemove.c:824-828`), where the wire
/// encoder, which compares raw bits, can see it.
fn vector_normalize(vector: Vec3) -> Vec3 {
    let length = vector.length();
    if length != 0.0 {
        vector * (1.0 / length)
    } else {
        vector
    }
}
