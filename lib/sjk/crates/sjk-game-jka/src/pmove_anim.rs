//! Protocol-compatible animation setters used by saber prediction.
//!
//! Priority, restart and timer rules mirror OpenJK
//! `codemp/game/bg_panimate.c:2553-2690,2760-2890,2940-3002`.

use std::sync::Arc;

use crate::pmove::MovementState;

/// `1 << FP_RAGE` in `fd.forcePowersActive` (`qcommon/q_shared.h:362-371`).
const FORCE_RAGE_BIT: u32 = 1 << 8;

/// `BOTH_RUN1`, `BOTH_WALK2`, `BOTH_WALKBACK1`, `BOTH_RUNBACK1`: the droids' substitution.
const BOTH_RUN1: u16 = 1_111;
const BOTH_WALK2: u16 = 1_103;
const BOTH_WALKBACK1: u16 = 1_134;
const BOTH_RUNBACK1: u16 = 1_136;
/// Apply an animation to the upper-body track.
pub const SETANIM_TORSO: u8 = 1;
/// Apply an animation to the lower-body track.
pub const SETANIM_LEGS: u8 = 2;
/// Apply an animation to both body tracks.
pub const SETANIM_BOTH: u8 = SETANIM_TORSO | SETANIM_LEGS;
/// Replace a lower-priority animation.
pub const SETANIM_FLAG_OVERRIDE: u8 = 1;
/// Hold the selected animation for its duration.
pub const SETANIM_FLAG_HOLD: u8 = 2;
/// Restart even when the animation number is unchanged.
pub const SETANIM_FLAG_RESTART: u8 = 4;
/// Hold for one frame less, matching codemp's transition timing.
pub const SETANIM_FLAG_HOLDLESS: u8 = 8;

/// One animation's wire-relevant timing data.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AnimationTiming {
    /// Number of frames in the animation sequence.
    pub frame_count: u16,
    /// Absolute milliseconds between successive frames.
    pub frame_lerp_ms: u16,
}

impl AnimationTiming {
    /// Full codemp animation duration in milliseconds.
    pub fn length_ms(self) -> i32 {
        i32::from(self.frame_count) * i32::from(self.frame_lerp_ms)
    }

    fn holdless_ms(self) -> i32 {
        i32::from(self.frame_count.saturating_sub(1)) * i32::from(self.frame_lerp_ms)
    }
}

/// Animation-duration source injected by the model compatibility adapter.
pub trait AnimationLengths: Send + Sync + std::fmt::Debug {
    /// Look up the full duration for a protocol animation ordinal.
    fn length_ms(&self, animation: u16) -> Option<i32>;
    /// Look up the frame timing for a protocol animation ordinal.
    fn timing(&self, animation: u16) -> Option<AnimationTiming>;
    /// The animation's first frame in its model's skeleton file (`animation_t::firstFrame`),
    /// which a saber lock holds its frame in; unknown for a table without it.
    fn first_frame(&self, _animation: u16) -> Option<u32> {
        None
    }
}

/// Immutable timing table built once from an `animation.cfg`.
#[derive(Clone, Debug)]
pub struct AnimationLengthTable {
    timings: Box<[AnimationTiming]>,
    /// Each animation's first frame, where the table knows them.
    first_frames: Option<Box<[u32]>>,
}

impl AnimationLengthTable {
    /// Build an immutable ordinal-indexed timing table.
    pub fn new(timings: impl IntoIterator<Item = AnimationTiming>) -> Self {
        Self {
            timings: timings.into_iter().collect(),
            first_frames: None,
        }
    }

    /// The table with each animation's first frame, by ordinal.
    pub fn with_first_frames(mut self, first_frames: impl IntoIterator<Item = u32>) -> Self {
        self.first_frames = Some(first_frames.into_iter().collect());
        self
    }

    /// Build the protocol-ordinal timing table from a parsed `animation.cfg`.
    pub fn from_animation_config(config: &sjk_model::AnimationConfig) -> Self {
        let first_frames = (0..crate::legacy_animation_count()).map(|index| {
            crate::legacy_animation_name(index)
                .and_then(|name| config.get(name))
                .map_or(0, |sequence| sequence.first_frame as u32)
        });
        Self::new((0..crate::legacy_animation_count()).map(|index| {
            let sequence = crate::legacy_animation_name(index).and_then(|name| config.get(name));
            let Some(sequence) = sequence else {
                return AnimationTiming::default();
            };
            let frame_lerp = if sequence.frames_per_second < 0.0 {
                (1_000.0 / sequence.frames_per_second).floor().abs()
            } else {
                (1_000.0 / sequence.frames_per_second).ceil()
            };
            AnimationTiming {
                frame_count: sequence.frame_count as u16,
                frame_lerp_ms: frame_lerp as u16,
            }
        }))
        .with_first_frames(first_frames)
    }

    /// Number of ordinal slots in the table.
    pub fn len(&self) -> usize {
        self.timings.len()
    }

    /// Whether the table has no ordinal slots.
    pub fn is_empty(&self) -> bool {
        self.timings.is_empty()
    }
}

impl AnimationLengths for AnimationLengthTable {
    fn length_ms(&self, animation: u16) -> Option<i32> {
        self.timing(animation).map(AnimationTiming::length_ms)
    }

    fn timing(&self, animation: u16) -> Option<AnimationTiming> {
        self.timings
            .get(usize::from(animation))
            .copied()
            .filter(|timing| timing.frame_count != 0 && timing.frame_lerp_ms != 0)
    }

    fn first_frame(&self, animation: u16) -> Option<u32> {
        self.first_frames
            .as_ref()?
            .get(usize::from(animation))
            .copied()
    }
}

pub(crate) type SharedAnimationLengths = Arc<dyn AnimationLengths>;

/// Apply codemp's `PM_SetAnim` priority, flip and timer rules.
pub fn set_animation(
    state: &mut MovementState,
    parts: u8,
    animation: u16,
    flags: u8,
    lengths: &dyn AnimationLengths,
) {
    // `BG_SetAnim` (`bg_panimate.c:2943-2956`): a skeleton without the animation — a
    // droid's — walks where it would run or back up, and otherwise keeps what it has.
    let (animation, timing) = match lengths.timing(animation) {
        Some(timing) => (animation, timing),
        None if matches!(animation, BOTH_RUNBACK1 | BOTH_WALKBACK1 | BOTH_RUN1) => {
            match lengths.timing(BOTH_WALK2) {
                Some(timing) => (BOTH_WALK2, timing),
                None => return,
            }
        }
        None => return,
    };
    if timing.frame_count == 0 || timing.frame_lerp_ms == 0 {
        return;
    }
    let speed = saber_start_trans_anim(
        1.0,
        animation,
        state.weapon,
        i32::from(state.saber_anim_level),
        state.broken_limbs,
        state.saber_anim_speed_scales.0,
    );
    let rage = state.force_powers_active & FORCE_RAGE_BIT != 0;
    if flags & SETANIM_FLAG_OVERRIDE != 0 {
        if parts & SETANIM_TORSO != 0
            && (flags & SETANIM_FLAG_RESTART != 0 || state.torso_anim != animation)
        {
            state.torso_timer = 0;
        }
        if parts & SETANIM_LEGS != 0
            && (flags & SETANIM_FLAG_RESTART != 0 || state.legs_anim != animation)
        {
            state.legs_timer = 0;
        }
    }
    if parts & SETANIM_TORSO != 0
        && part_accepts(state.torso_anim, state.torso_timer, animation, flags)
    {
        start_torso(state, animation);
        if let Some(timer) = hold_timer(flags, timing, speed) {
            // `bg_panimate.c:2811-2814`: rage shortens every held torso clip.
            state.torso_timer = if rage {
                (f64::from(timer) / 1.7) as i32
            } else {
                timer
            };
        }
    }
    if parts & SETANIM_LEGS != 0
        && part_accepts(state.legs_anim, state.legs_timer, animation, flags)
    {
        start_legs(state, animation);
        if let Some(timer) = hold_timer(flags, timing, speed) {
            // The rage/speed legs divisors apply to locomotion clips only
            // (`bg_panimate.c:2845-2856`), which no caller holds.
            state.legs_timer = timer;
        }
    }
}

/// The two gates of one track in `BG_SetAnimFinal` (`bg_panimate.c:2777-2787`): an
/// animation already running is left alone unless restarted, and a running timer
/// protects its animation unless overridden.
fn part_accepts(current: u16, timer: i32, animation: u16, flags: u8) -> bool {
    (flags & SETANIM_FLAG_RESTART != 0 || current != animation)
        && (flags & SETANIM_FLAG_OVERRIDE != 0 || !(timer > 0 || timer == -1))
}

/// The hold timer of `BG_SetAnimFinal` (`bg_panimate.c:2791-2810`), if one is set.
fn hold_timer(flags: u8, timing: AnimationTiming, speed: f32) -> Option<i32> {
    if flags & SETANIM_FLAG_HOLD == 0 {
        return None;
    }
    if flags & SETANIM_FLAG_HOLDLESS == 0 {
        return Some(timing.length_ms());
    }
    // Only the HOLDLESS path scales by the transition speed.
    let duration = timing.holdless_ms();
    let speed_difference = duration as f32 - duration as f32 * speed;
    let adjusted = duration + speed_difference as i32;
    Some(if adjusted > 1 {
        adjusted - 1
    } else {
        i32::from(timing.frame_lerp_ms)
    })
}

/// `PM_DEAD`: from here on a player's animations are the game's, not movement's.
const PM_DEAD: u8 = 5;

/// `BG_StartLegsAnim` (`bg_panimate.c:2553-2586`): start a legs animation unless a
/// held one is running, toggling the restart flag when it is the one already playing.
///
/// The game build also toggles when the player's *entity* still shows this
/// animation: it was left and re-entered within one server frame, which a client
/// could not see otherwise. [`MovementState::entity_animations`] carries that.
pub(crate) fn start_legs(state: &mut MovementState, animation: u16) {
    if state.movement_type >= PM_DEAD || state.legs_timer > 0 {
        return;
    }
    if state.legs_anim == animation
        || state
            .entity_animations
            .is_some_and(|[legs, _]| legs == animation)
    {
        state.legs_flip = !state.legs_flip;
    }
    state.legs_anim = animation;
}

/// `PM_ContinueLegsAnim` (`bg_panimate.c:2588-2597`): switch the legs to a looping
/// animation unless it is already playing or a held one is running.
pub(crate) fn continue_legs(state: &mut MovementState, animation: u16) {
    if state.legs_anim != animation && state.legs_timer <= 0 {
        start_legs(state, animation);
    }
}

/// `PM_ForceLegsAnim` (`bg_panimate.c:2599-2616`): drop the running hold and start a
/// legs animation, unless that would cut a special jump or a roll short.
pub(crate) fn force_legs(state: &mut MovementState, animation: u16) {
    use crate::pmove_roll_anim::{in_roll, special_jump};
    let held = state.legs_timer > 0;
    if held
        && (special_jump(state.legs_anim) && !special_jump(animation)
            || in_roll(state.legs_anim) && !in_roll(animation))
    {
        return;
    }
    state.legs_timer = 0;
    start_legs(state, animation);
}

/// `BG_StartTorsoAnim` (`bg_panimate.c:2626-2645`); see [`start_legs`] for the
/// entity rule. Unlike the legs, a running torso timer does not stop it.
pub(crate) fn start_torso(state: &mut MovementState, animation: u16) {
    if state.movement_type >= PM_DEAD {
        return;
    }
    if state.torso_anim == animation
        || state
            .entity_animations
            .is_some_and(|[_, torso]| torso == animation)
    {
        state.torso_flip = !state.torso_flip;
    }
    state.torso_anim = animation;
}

/// `BG_SaberStartTransAnim` (`bg_panimate.c:2693-2751`) on `speed`: a saber attack or
/// special (`BOTH_A1_T__B_`..=`BOTH_ROLL_STAB`) at its hilts' `animSpeedScale`; a
/// transition faster in the fast style and slower in the strong one, slower still with a
/// broken arm (`BROKENLIMB_RARM` is bit 2, `BROKENLIMB_LARM` bit 1); any other saber
/// animation (`PM_InSaberAnim`: `BOTH_A1_T__B_`..=`BOTH_H1_S1_BR`) slower with one.
pub fn saber_start_trans_anim(
    mut speed: f32,
    animation: u16,
    weapon: u8,
    style: i32,
    broken_limbs: u8,
    hilt_scales: [f32; 2],
) -> f32 {
    const WP_SABER: u8 = 3;
    const RIGHT_ARM: u8 = 1 << 2;
    const LEFT_ARM: u8 = 1 << 1;
    if (126..=914).contains(&animation) && weapon == WP_SABER {
        for scale in hilt_scales {
            if scale != 1.0 {
                speed *= scale;
            }
        }
    }
    let transition = (133..=174).contains(&animation)
        || (210..=251).contains(&animation)
        || (287..=328).contains(&animation);
    if transition {
        if style == 1 {
            speed *= 1.5;
        } else if style == 3 {
            speed *= 0.75;
        }
    }
    if transition || (broken_limbs != 0 && (126..=689).contains(&animation)) {
        if broken_limbs & RIGHT_ARM != 0 {
            speed *= 0.5;
        } else if broken_limbs & LEFT_ARM != 0 {
            speed *= 0.65;
        }
    }
    speed
}

/// `G_SetAnim`/`BG_SetAnim` on a player's wire state (`g_utils.c:489-518`): the
/// animation set on the half or halves asked, by the same rules as a move's, and the
/// animations, their timers and flips written back. `hilt_scales` are the sabers'
/// `animSpeedScale` (`BG_MySaber`).
pub fn animate(
    state: &mut sjk_protocol::PlayerState,
    parts: u8,
    animation: u16,
    flags: u8,
    lengths: &dyn AnimationLengths,
    hilt_scales: [f32; 2],
) {
    let mut movement = MovementState::from_player_state(state);
    movement.saber_anim_speed_scales = crate::pmove::HiltSpeedScales(hilt_scales);
    set_animation(&mut movement, parts, animation, flags, lengths);
    for (index, value) in [
        (13, u32::from(movement.legs_anim)),
        (15, u32::from(movement.torso_anim)),
        (21, movement.legs_timer as u32),
        (20, movement.torso_timer as u32),
        (69, u32::from(movement.legs_flip)),
        (55, u32::from(movement.torso_flip)),
    ] {
        state.set_raw_field(index, value);
    }
}
