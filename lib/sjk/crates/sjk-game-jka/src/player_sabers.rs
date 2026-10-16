//! A player's two sabers as the game keeps them: `client->saber[MAX_SABERS]` and the
//! names `pers.saber1`/`pers.saber2` other clients are told (`st`, `st2`).
//!
//! They are set by name from the userinfo twice in a player's stay: on its first
//! connect (`ClientUserinfoChanged`, `g_client.c:2267-2272`), and at a spawn whose
//! userinfo names others (`ClientSpawn`, `g_client.c:3109-3160`), which also settles the
//! stance the new sabers bring.

use crate::client_spawn::SaberKit;
use crate::saber_definition::{
    DEFAULT_SABER, MAX_QPATH, SABER_NAME_LENGTH, SFL_TWO_HANDED, SaberDefinition,
    SaberDefinitionError, SaberParms, SaberParseHost, set_saber, truncated,
};
use crate::saber_info::Sabers;
use crate::userinfo::SaberCatalog;

/// The two hands.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerSabers {
    pub hands: [SaberDefinition; 2],
}

impl Default for PlayerSabers {
    /// A cleared client: nothing set yet.
    fn default() -> Self {
        Self {
            hands: [SaberDefinition::empty(), SaberDefinition::empty()],
        }
    }
}

/// A parse that registers nothing and draws nothing, for questions about a saber that
/// do not set it.
struct Asking;

impl SaberParseHost for Asking {
    fn sound_index(&mut self, _: &[u8]) -> u16 {
        0
    }
    fn irand(&mut self, low: i32, _: i32) -> i32 {
        low
    }
}

impl SaberCatalog for SaberParms {
    /// Usable when it is not campaign-only and its own block is found under the name
    /// asked for (an unknown name falls back to the default saber, under its name).
    fn player_saber(&self, name: &[u8]) -> Option<bool> {
        if !self.valid_for_player_in_mp(name) {
            return None;
        }
        match self.parse(name, &mut Asking) {
            Ok((true, saber)) if saber.name == truncated(name, SABER_NAME_LENGTH) => {
                Some(saber.flags & SFL_TWO_HANDED != 0)
            }
            _ => None,
        }
    }
}

impl PlayerSabers {
    /// The style these sabers hold a player to without the saber-attack holocron
    /// (`HolocronUpdate`): dual for two, staff for a two-handed one, else medium.
    pub fn base_style(&self) -> u8 {
        // `SS_DUAL`, `SS_STAFF`, `SS_MEDIUM`: `HolocronUpdate`'s own test.
        if !self.hands[0].model.is_empty() && !self.hands[1].model.is_empty() {
            4
        } else if self.hands[0].flags & crate::saber_definition::SFL_TWO_HANDED != 0 {
            5
        } else {
            2
        }
    }

    /// The Force powers these sabers forbid while a blade is lit (`WP_ForcePowerUsable`,
    /// `w_force.c:754-786`): the first saber's `forceRestrict`, and — the reference testing
    /// the first saber's model where it means the second's — the second's, a removed one's
    /// being none.
    pub fn force_restrictions(&self) -> u32 {
        let [first, second] = &self.hands;
        let mut forbidden = 0;
        if first.flags & SFL_TWO_HANDED != 0 || first.is_held() {
            forbidden |= first.force_restrictions as u32;
        }
        if first.is_held() {
            forbidden |= second.force_restrictions as u32;
        }
        forbidden
    }

    /// The sabers' `animSpeedScale` and `moveSpeedScale` as `BG_MySaber` finds them: the
    /// first saber's, and the second's only where that hand holds one; 1 otherwise.
    pub fn speed_scales(&self) -> ([f32; 2], [f32; 2]) {
        let [first, second] = &self.hands;
        let held = |saber: &SaberDefinition, scale: f32| if saber.is_held() { scale } else { 1.0 };
        (
            [
                held(first, first.anim_speed_scale),
                held(second, second.anim_speed_scale),
            ],
            [
                held(first, first.move_speed_scale),
                held(second, second.move_speed_scale),
            ],
        )
    }

    /// The first saber may block missiles actively (no `SFL_NOT_ACTIVE_BLOCKING`,
    /// `w_saber.c:5509`).
    pub fn actively_blocks(&self) -> bool {
        self.hands[0].flags & (1 << 3) == 0
    }

    /// Whether these lock at `saberHolstered` `holstered` (`w_saber.c:1563-1580`): not
    /// with a first saber, or a fully lit second one, marked `SFL_NOT_LOCKABLE`.
    pub fn lockable(&self, holstered: u8) -> bool {
        const SFL_NOT_LOCKABLE: u32 = 1 << 0;
        let [first, second] = &self.hands;
        first.flags & SFL_NOT_LOCKABLE == 0
            && !(second.is_held() && holstered == 0 && second.flags & SFL_NOT_LOCKABLE != 0)
    }

    /// The `lockBonus` a lock's attack press adds (`g_active.c:2970-2975`): the first
    /// saber's, and a fully lit second saber's.
    pub fn lock_bonus(&self, holstered: u8) -> i32 {
        let [first, second] = &self.hands;
        first.lock_bonus
            + if second.is_held() && holstered == 0 {
                second.lock_bonus
            } else {
                0
            }
    }

    /// `disarmChance` against a player holding these at `saberHolstered` `holstered`
    /// (`w_saber.c:6755-6764`): 1, the first saber's `disarmBonus`, and — with
    /// `g_fixSaberDisarmBonus`, on by default — a fully lit second saber's.
    pub fn disarm_chance(&self, holstered: u8) -> i32 {
        let [first, second] = &self.hands;
        1 + first.disarm_bonus[0]
            + if second.is_held() && holstered == 0 {
                second.disarm_bonus[0]
            } else {
                0
            }
    }

    /// The knockback scales a saber blow's `DAMAGE_SABER_KNOCKBACK*` flags pick
    /// ([`crate::damage::Attacker::saber_knockback`]): each saber's, each blade style's.
    pub fn knockback_scales(&self) -> [f32; 4] {
        let [first, second] = &self.hands;
        [
            first.knockback_scale[0],
            first.knockback_scale[1],
            second.knockback_scale[0],
            second.knockback_scale[1],
        ]
    }

    /// The stock saber in the first hand, none in the second, as a player without saber
    /// definitions holds them; its sounds are left unregistered (index 0).
    pub fn stock() -> Self {
        Self {
            hands: [
                SaberDefinition::defaults(&mut Asking),
                SaberDefinition::removed(&mut Asking),
            ],
        }
    }

    /// Whether any saber was ever set in these hands.
    fn ever_set(&self) -> bool {
        !self.hands[0].name.is_empty() || self.hands[0].is_held()
    }

    /// `pers.saber1` and `pers.saber2`: the first saber's name, the second's or `none`.
    /// Both empty before anything was set.
    pub fn names(&self) -> (Vec<u8>, Vec<u8>) {
        if !self.ever_set() {
            return (Vec::new(), Vec::new());
        }
        let first = if self.hands[0].is_held() {
            self.hands[0].name.clone()
        } else {
            DEFAULT_SABER.to_vec()
        };
        let second = if self.hands[1].is_held() {
            self.hands[1].name.clone()
        } else {
            b"none".to_vec()
        };
        (first, second)
    }

    /// `G_SetSaber` outside a siege class's hold: hand `hand` set to the saber called
    /// `name`, cut to a path's length; the first saber cannot be removed this way.
    pub fn set(
        &mut self,
        parms: &SaberParms,
        hand: usize,
        name: &[u8],
        host: &mut impl SaberParseHost,
    ) -> Result<(), SaberDefinitionError> {
        let name = truncated(name, MAX_QPATH);
        let removes = name.eq_ignore_ascii_case(b"none") || name.eq_ignore_ascii_case(b"remove");
        let name = if hand == 0 && removes {
            DEFAULT_SABER.to_vec()
        } else {
            name
        };
        set_saber(parms, &mut self.hands, hand, &name, host)
    }

    /// `ClientUserinfoChanged`'s first connect: both hands from the userinfo's
    /// `saber1` and `saber2`.
    pub fn connect(
        &mut self,
        parms: &SaberParms,
        saber1: &[u8],
        saber2: &[u8],
        host: &mut impl SaberParseHost,
    ) -> Result<(), SaberDefinitionError> {
        if self.names().0.is_empty() || self.names().1.is_empty() {
            self.set(parms, 0, saber1, host)?;
            self.set(parms, 1, saber2, host)?;
        }
        Ok(())
    }

    /// `ClientSpawn`'s check: each hand the userinfo names differently (any case), or
    /// that holds nothing usable, is set again. The kit the sabers now make when any
    /// was, for the stance it brings; `None` when nothing changed.
    pub fn spawn_check(
        &mut self,
        parms: &SaberParms,
        saber1: &[u8],
        saber2: &[u8],
        host: &mut impl SaberParseHost,
    ) -> Result<Option<SaberKit>, SaberDefinitionError> {
        let mut changed = false;
        for (hand, wanted) in [(0, saber1), (1, saber2)] {
            let names = self.names();
            let current = if hand == 0 { names.0 } else { names.1 };
            if !wanted.eq_ignore_ascii_case(&current)
                || current.is_empty()
                || !self.hands[0].is_held()
            {
                self.set(parms, hand, wanted, host)?;
                changed = true;
            }
        }
        Ok(changed.then(|| self.kit()))
    }

    /// Two sabers held, one two-handed, or one single.
    pub fn kit(&self) -> SaberKit {
        if self.hands[0].is_held() && self.hands[1].is_held() {
            SaberKit::Dual
        } else if self.hands[0].flags & SFL_TWO_HANDED != 0 {
            SaberKit::Staff
        } else {
            SaberKit::Single
        }
    }

    /// What the sabers say about their wielder's moves (`BG_MySaber`).
    pub fn movement(&self) -> Sabers {
        let held = |saber: &SaberDefinition| saber.is_held().then(|| saber.movement());
        Sabers {
            first: held(&self.hands[0]),
            second: held(&self.hands[1]),
        }
    }
}
