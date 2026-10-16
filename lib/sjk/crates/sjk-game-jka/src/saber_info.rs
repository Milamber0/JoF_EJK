//! What a saber's definition changes about how it is swung (`saberInfo_t`'s movement
//! half, `codemp/game/bg_public.h`): the special moves it overrides or cancels, the
//! poses it plays instead of the style's, and the flags that forbid moves. The stock
//! saber (`WP_SaberSetDefaults`, `bg_saberLoad.c:401-500`) overrides nothing; saber
//! definitions (`WP_SaberParseParms`) are not read yet, so every player carries it.

/// `LS_INVALID`: a move the saber leaves to the style.
pub const LS_INVALID: i32 = -1;
/// `SFL_NO_STABDOWN`, `SFL_NO_CARTWHEELS`, `SFL_NO_KICKS`, `SFL_NO_MIRROR_ATTACKS`,
/// `SFL_NO_ROLL_STAB`: moves the saber does not allow.
pub const SFL_NO_STABDOWN: u32 = 1 << 12;
pub const SFL_NO_CARTWHEELS: u32 = 1 << 18;
pub const SFL_NO_KICKS: u32 = 1 << 19;
pub const SFL_NO_MIRROR_ATTACKS: u32 = 1 << 20;
pub const SFL_NO_ROLL_STAB: u32 = 1 << 21;
/// `SFL_NOT_THROWABLE`, `SFL_SINGLE_BLADE_THROWABLE`: which sabers may be thrown.
pub const SFL_NOT_THROWABLE: u32 = 1 << 1;
pub const SFL_SINGLE_BLADE_THROWABLE: u32 = 1 << 5;

/// A saber's say in its wielder's moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaberInfo {
    /// `kataMove`, `lungeAtkMove`, `jumpAtkUpMove`, `jumpAtkFwdMove`, `jumpAtkBackMove`,
    /// `jumpAtkRightMove`, `jumpAtkLeftMove`: a move instead of the style's, `LS_NONE` (0)
    /// to cancel it, [`LS_INVALID`] to leave it.
    pub kata_move: i32,
    pub lunge_move: i32,
    pub jump_up_move: i32,
    pub jump_forward_move: i32,
    pub jump_back_move: i32,
    pub jump_right_move: i32,
    pub jump_left_move: i32,
    /// `readyAnim`, `drawAnim`, `putawayAnim`: -1 for the style's.
    pub ready_anim: i32,
    pub draw_anim: i32,
    pub putaway_anim: i32,
    /// `saberFlags`.
    pub flags: u32,
    /// `lockBonus`: weight a lock's press gains.
    pub lock_bonus: i32,
}

impl SaberInfo {
    /// The stock saber: nothing overridden, nothing forbidden.
    pub const STOCK: Self = Self {
        kata_move: LS_INVALID,
        lunge_move: LS_INVALID,
        jump_up_move: LS_INVALID,
        jump_forward_move: LS_INVALID,
        jump_back_move: LS_INVALID,
        jump_right_move: LS_INVALID,
        jump_left_move: LS_INVALID,
        ready_anim: -1,
        draw_anim: -1,
        putaway_anim: -1,
        flags: 0,
        lock_bonus: 0,
    };
}

/// A player's sabers as `BG_MySaber` finds them: the first, and a second one only when it
/// carries two (`saber[1].model[0]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sabers {
    pub first: Option<SaberInfo>,
    pub second: Option<SaberInfo>,
}

impl Sabers {
    /// One stock saber.
    pub const STOCK: Self = Self {
        first: Some(SaberInfo::STOCK),
        second: None,
    };

    /// Whether either saber has `flag`.
    pub fn any_flag(&self, flag: u32) -> bool {
        self.first.is_some_and(|saber| saber.flags & flag != 0)
            || self.second.is_some_and(|saber| saber.flags & flag != 0)
    }

    /// A special move as the sabers have it (`PM_SaberLungeAttackMove` and its kin): the
    /// first saber's override, else the second's; cancelled (`LS_A_T2B` instead) by either
    /// cancelling it; `None` for the style's own.
    pub fn special(&self, pick: impl Fn(&SaberInfo) -> i32, cancelled: u16) -> Option<u16> {
        for saber in [self.first, self.second].into_iter().flatten() {
            let chosen = pick(&saber);
            if chosen != LS_INVALID && chosen != 0 {
                return Some(chosen as u16);
            }
        }
        for saber in [self.first, self.second].into_iter().flatten() {
            if pick(&saber) == 0 {
                return Some(cancelled);
            }
        }
        None
    }
}

impl Default for Sabers {
    fn default() -> Self {
        Self::STOCK
    }
}
