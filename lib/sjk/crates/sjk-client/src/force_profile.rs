//! BaseJKA Force-profile encoding and legalization policy.
//!
//! The power order is `forcePowers_t` from
//! `codemp/qcommon/q_shared.h:361-382`. Cost, side, and legalization data
//! mirror `codemp/game/bg_misc.c:165-242,416-691`.

use std::fmt;

/// Number of powers in BaseJKA's `forcePowers_t`.
pub const FORCE_POWER_COUNT: usize = 18;

const MASTERY_POINTS: [u16; 8] = [0, 5, 10, 20, 30, 50, 75, 100];
const POWER_COSTS: [[u8; 4]; FORCE_POWER_COUNT] = [
    [0, 2, 4, 6],
    [0, 0, 2, 6],
    [0, 2, 4, 6],
    [0, 1, 3, 6],
    [0, 1, 3, 6],
    [0, 4, 6, 8],
    [0, 1, 3, 6],
    [0, 2, 5, 8],
    [0, 4, 6, 8],
    [0, 2, 5, 8],
    [0, 1, 3, 6],
    [0, 1, 3, 6],
    [0, 1, 3, 6],
    [0, 2, 4, 6],
    [0, 2, 5, 8],
    [0, 1, 5, 8],
    [0, 1, 5, 8],
    [0, 4, 6, 8],
];
const POWER_SIDES: [u8; FORCE_POWER_COUNT] = [1, 0, 0, 0, 0, 1, 2, 2, 2, 1, 1, 1, 2, 2, 0, 0, 0, 0];
const LEVITATION: usize = 1;
const TEAM_HEAL: usize = 11;
const TEAM_FORCE: usize = 12;
const SABER_OFFENSE: usize = 15;
const SABER_DEFENSE: usize = 16;
const SABER_THROW: usize = 17;

/// Force allegiance encoded in the second field of `forcepowers`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ForceSide {
    /// Light-side powers are permitted.
    Light = 1,
    /// Dark-side powers are permitted.
    Dark = 2,
}

impl ForceSide {
    fn from_legacy(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Light),
            2 => Some(Self::Dark),
            _ => None,
        }
    }
}

/// One Force power in the exact `forcePowers_t` order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ForcePower {
    Heal,
    Levitation,
    Speed,
    Push,
    Pull,
    Telepathy,
    Grip,
    Lightning,
    Rage,
    Protect,
    Absorb,
    TeamHeal,
    TeamForce,
    Drain,
    See,
    SaberOffense,
    SaberDefense,
    SaberThrow,
}

impl ForcePower {
    /// All powers in their serialized order.
    pub const ALL: [Self; FORCE_POWER_COUNT] = [
        Self::Heal,
        Self::Levitation,
        Self::Speed,
        Self::Push,
        Self::Pull,
        Self::Telepathy,
        Self::Grip,
        Self::Lightning,
        Self::Rage,
        Self::Protect,
        Self::Absorb,
        Self::TeamHeal,
        Self::TeamForce,
        Self::Drain,
        Self::See,
        Self::SaberOffense,
        Self::SaberDefense,
        Self::SaberThrow,
    ];

    const fn index(self) -> usize {
        self as usize
    }

    /// Side this power belongs to (`forcePowerDarkLight` in `bg_misc.c`),
    /// or `None` for a neutral power.
    pub const fn side(self) -> Option<ForceSide> {
        match POWER_SIDES[self.index()] {
            1 => Some(ForceSide::Light),
            2 => Some(ForceSide::Dark),
            _ => None,
        }
    }

    /// Whether this is a team power, which `BG_LegalizedForcePowers` strips
    /// below `GT_TEAM`.
    pub const fn is_team_power(self) -> bool {
        matches!(self, Self::TeamHeal | Self::TeamForce)
    }

    /// Points level `level` (1 to 3) of this power costs in
    /// `bgForcePowerCost`, the figure the legacy menu prints on its star;
    /// 0 for any other level. See [`Self::level_is_free`] for the levels
    /// granted without spending it.
    pub const fn level_cost(self, level: u8) -> u8 {
        if level == 0 || level > 3 {
            return 0;
        }
        POWER_COSTS[self.index()][level as usize]
    }

    /// Whether `level` comes without points: Jump's first level always,
    /// Saber Offense's and Defense's under free saber.
    pub const fn level_is_free(self, level: u8, free_saber: bool) -> bool {
        level == 1
            && (matches!(self, Self::Levitation)
                || free_saber && matches!(self, Self::SaberOffense | Self::SaberDefense))
    }
}

/// Points a mastery rank (0 to 7, higher ranks clamp) grants to spend
/// (`forceMasteryPoints`, `bg_misc.c`).
pub const fn mastery_points(rank: u8) -> u16 {
    let rank = if rank > 7 { 7 } else { rank };
    MASTERY_POINTS[rank as usize]
}

/// Parsed player-selected Force allocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForceAllocation {
    /// Requested mastery rank, clamped by the server during legalization.
    pub rank: u8,
    /// Requested Force side.
    pub side: ForceSide,
    /// Power levels in [`ForcePower::ALL`] order.
    pub levels: [u8; FORCE_POWER_COUNT],
}

impl Default for ForceAllocation {
    fn default() -> Self {
        Self::parse("7-1-032330000000001333").expect("stock Force profile is valid")
    }
}

impl ForceAllocation {
    /// Parse the legacy `<rank>-<side>-<18 levels>` representation.
    pub fn parse(text: &str) -> Result<Self, ForceProfileError> {
        let mut fields = text.split('-');
        let rank = fields
            .next()
            .ok_or(ForceProfileError::Malformed)?
            .parse::<u8>()
            .map_err(|_| ForceProfileError::Malformed)?;
        let side = fields
            .next()
            .ok_or(ForceProfileError::Malformed)?
            .parse::<u8>()
            .ok()
            .and_then(ForceSide::from_legacy)
            .ok_or(ForceProfileError::InvalidSide)?;
        let digits = fields.next().ok_or(ForceProfileError::Malformed)?;
        if fields.next().is_some() || digits.len() != FORCE_POWER_COUNT {
            return Err(ForceProfileError::Malformed);
        }
        let mut levels = [0; FORCE_POWER_COUNT];
        for (slot, digit) in levels.iter_mut().zip(digits.bytes()) {
            if !(b'0'..=b'3').contains(&digit) {
                return Err(ForceProfileError::InvalidLevel);
            }
            *slot = digit - b'0';
        }
        Ok(Self { rank, side, levels })
    }

    /// Encode the exact legacy userinfo representation.
    pub fn encode(&self) -> String {
        let mut result = format!("{}-{}-", self.rank, self.side as u8);
        result.reserve(FORCE_POWER_COUNT);
        for level in self.levels {
            result.push(char::from(b'0' + level.min(3)));
        }
        result
    }

    /// Points consumed under BaseJKA's cumulative per-level cost table.
    pub fn used_points(&self, free_saber: bool) -> u16 {
        used_points(&self.levels, free_saber)
    }

    /// Points remaining at this profile's rank.
    pub fn remaining_points(&self, free_saber: bool) -> u16 {
        MASTERY_POINTS[usize::from(self.rank.min(7))].saturating_sub(self.used_points(free_saber))
    }

    /// Bit mask of powers whose legalized level is non-zero.
    pub fn known_mask(&self) -> u32 {
        self.levels
            .iter()
            .enumerate()
            .fold(0_u32, |mask, (index, level)| {
                mask | (u32::from(*level != 0) << index)
            })
    }

    /// Spend one level when side, level, and point constraints permit it.
    pub fn spend(&mut self, power: ForcePower, free_saber: bool) -> bool {
        let index = power.index();
        let level = self.levels[index];
        if level >= 3 || (POWER_SIDES[index] != 0 && POWER_SIDES[index] != self.side as u8) {
            return false;
        }
        let cost = POWER_COSTS[index][usize::from(level + 1)];
        let free_level = level == 0
            && (index == LEVITATION
                || free_saber && matches!(index, SABER_OFFENSE | SABER_DEFENSE));
        if !free_level && u16::from(cost) > self.remaining_points(free_saber) {
            return false;
        }
        self.levels[index] += 1;
        true
    }

    /// Refund one purchased level while preserving BaseJKA's free minima.
    pub fn refund(&mut self, power: ForcePower, free_saber: bool) -> bool {
        let index = power.index();
        let minimum = u8::from(
            index == LEVITATION || free_saber && matches!(index, SABER_OFFENSE | SABER_DEFENSE),
        );
        if self.levels[index] <= minimum {
            return false;
        }
        self.levels[index] -= 1;
        true
    }
}

/// Server constraints supplied to `BG_LegalizedForcePowers`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ForceLegalizeRules {
    /// Server mastery ceiling in the range zero through seven.
    pub max_rank: u8,
    /// Whether offense and defense level one cost no points.
    pub free_saber: bool,
    /// Team-enforced Force side, when force-based teams are active.
    pub team_side: Option<ForceSide>,
    /// Legacy gametype number used to permit or remove team powers.
    pub gametype: i32,
    /// `g_forcePowerDisable` bit mask.
    pub disabled_mask: u32,
}

impl Default for ForceLegalizeRules {
    fn default() -> Self {
        Self {
            max_rank: 7,
            free_saber: false,
            team_side: None,
            gametype: 0,
            disabled_mask: 0,
        }
    }
}

impl ForceLegalizeRules {
    /// The rules a profile the client sends must meet: these, but keeping the
    /// powers `g_forcePowerDisable` turns off.
    ///
    /// `BG_LegalizedForcePowers` (`bg_misc.c`) zeroes a disabled power in the
    /// server's own copy before it counts points and without calling the
    /// profile illegal, so `WP_InitForcePowers` never parks a player for one.
    /// Kept in the sent profile, they cost points here (so the profile still
    /// fits once a mod lets them back) and are there for a mod that restores
    /// them, as jaPRO does in Force duels for its own disable list.
    pub const fn for_sent_profile(self) -> Self {
        Self {
            disabled_mask: 0,
            ..self
        }
    }
}

/// Result of BaseJKA force-profile legalization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalizedForcePowers {
    /// Server-normalized rank, side, and levels.
    pub allocation: ForceAllocation,
    /// Whether the input fit the point budget and had a valid side.
    pub was_legal: bool,
}

/// Mirror `BG_LegalizedForcePowers` (`bg_misc.c:416-691`).
pub fn legalize_force_powers(input: &str, rules: ForceLegalizeRules) -> LegalizedForcePowers {
    let max_rank = rules.max_rank.min(7);
    let fallback = "5-1-000000000000000000";
    let source = if input.len() >= 128 { fallback } else { input };
    let mut valid = input.len() < 128;
    let mut sections = source.splitn(3, '-');
    let _ignored_rank = sections.next();
    let side_value = sections
        .next()
        .and_then(|value| value.parse::<u8>().ok())
        .and_then(ForceSide::from_legacy);
    let mut side = side_value.unwrap_or(ForceSide::Dark);
    valid &= side_value.is_some();
    if let Some(team_side) = rules.team_side {
        side = team_side;
    }
    let mut levels = [0_u8; FORCE_POWER_COUNT];
    if let Some(digits) = sections.next() {
        for (slot, digit) in levels.iter_mut().zip(digits.bytes()) {
            if !(b'0'..=b'3').contains(&digit) {
                break;
            }
            *slot = digit - b'0';
        }
    }
    for (index, level) in levels.iter_mut().enumerate() {
        if *level > 0 && POWER_SIDES[index] != 0 && POWER_SIDES[index] != side as u8 {
            *level = 0;
        }
        if *level > 0 && rules.disabled_mask & (1 << index) != 0 {
            *level = 0;
        }
    }
    if rules.gametype < 6 {
        levels[TEAM_HEAL] = 0;
        levels[TEAM_FORCE] = 0;
    }
    let allowed = MASTERY_POINTS[usize::from(max_rank)];
    let mut used = used_points(&levels, rules.free_saber);
    if used > allowed {
        valid = false;
        trim_to_budget(&mut levels, rules.free_saber, allowed, &mut used);
    }
    if rules.free_saber {
        levels[SABER_OFFENSE] = levels[SABER_OFFENSE].max(1);
        levels[SABER_DEFENSE] = levels[SABER_DEFENSE].max(1);
    }
    levels[LEVITATION] = levels[LEVITATION].max(1);
    if rules.disabled_mask != 0 {
        if rules.disabled_mask & (1 << LEVITATION) != 0 {
            levels[LEVITATION] = 1;
        }
        if rules.disabled_mask & (1 << SABER_OFFENSE) != 0 {
            levels[SABER_OFFENSE] = 3;
        }
        if rules.disabled_mask & (1 << SABER_DEFENSE) != 0 {
            levels[SABER_DEFENSE] = 3;
        }
    }
    if levels[SABER_OFFENSE] == 0 {
        levels[SABER_DEFENSE] = 0;
        levels[SABER_THROW] = 0;
    }
    LegalizedForcePowers {
        allocation: ForceAllocation {
            rank: max_rank,
            side,
            levels,
        },
        was_legal: valid,
    }
}

fn used_points(levels: &[u8; FORCE_POWER_COUNT], free_saber: bool) -> u16 {
    levels
        .iter()
        .enumerate()
        .map(|(index, level)| {
            (1..=*level)
                .map(|rank| {
                    let free = rank == 1
                        && (index == LEVITATION
                            || free_saber && matches!(index, SABER_OFFENSE | SABER_DEFENSE));
                    if free {
                        0
                    } else {
                        u16::from(POWER_COSTS[index][usize::from(rank)])
                    }
                })
                .sum::<u16>()
        })
        .sum()
}

fn trim_to_budget(
    levels: &mut [u8; FORCE_POWER_COUNT],
    free_saber: bool,
    allowed: u16,
    used: &mut u16,
) {
    let minimum = usize::from(free_saber);
    for cycle in 2..=(FORCE_POWER_COUNT + 2) {
        for index in 0..FORCE_POWER_COUNT {
            if *used <= allowed || levels[index] == 0 || usize::from(levels[index]) >= cycle {
                continue;
            }
            let target = if index == SABER_OFFENSE
                && (usize::from(levels[SABER_DEFENSE]) > minimum || levels[SABER_THROW] > 0)
            {
                if levels[SABER_THROW] > 0 {
                    SABER_THROW
                } else {
                    SABER_DEFENSE
                }
            } else {
                index
            };
            lower_power(levels, target, free_saber, allowed, used);
        }
        if *used <= allowed {
            return;
        }
    }
    levels.fill(0);
    levels[LEVITATION] = 1;
    if free_saber {
        levels[SABER_OFFENSE] = 1;
        levels[SABER_DEFENSE] = 1;
    }
    *used = 0;
}

fn lower_power(
    levels: &mut [u8; FORCE_POWER_COUNT],
    index: usize,
    free_saber: bool,
    allowed: u16,
    used: &mut u16,
) {
    while levels[index] > 0 && *used > allowed {
        let level = levels[index];
        let protected = level == 1
            && (index == LEVITATION
                || free_saber && matches!(index, SABER_OFFENSE | SABER_DEFENSE));
        if protected {
            break;
        }
        *used -= u16::from(POWER_COSTS[index][usize::from(level)]);
        levels[index] -= 1;
    }
}

/// Force-profile parse failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForceProfileError {
    /// The string did not have the legacy rank-side-level structure.
    Malformed,
    /// The side was neither light nor dark.
    InvalidSide,
    /// A level digit was outside zero through three.
    InvalidLevel,
}

impl fmt::Display for ForceProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid forcepowers: {self:?}")
    }
}

impl std::error::Error for ForceProfileError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published costs add up to what spending charges.
    #[test]
    fn level_costs_match_spending() {
        assert_eq!(ForcePower::Push.level_cost(3), 6);
        assert_eq!(ForcePower::Telepathy.level_cost(1), 4);
        assert_eq!(ForcePower::Levitation.level_cost(1), 0);
        assert_eq!(ForcePower::Heal.level_cost(0), 0);
        assert_eq!(ForcePower::Heal.level_cost(4), 0);
        assert!(ForcePower::Levitation.level_is_free(1, false));
        assert!(!ForcePower::SaberOffense.level_is_free(1, false));
        assert!(ForcePower::SaberDefense.level_is_free(1, true));
        assert!(!ForcePower::SaberDefense.level_is_free(2, true));
        assert_eq!(mastery_points(7), 100);
        assert_eq!(mastery_points(9), 100);
        for free_saber in [false, true] {
            for power in ForcePower::ALL {
                let mut levels = [0; FORCE_POWER_COUNT];
                levels[power.index()] = 3;
                let charged: u16 = (1..=3)
                    .filter(|level| !power.level_is_free(*level, free_saber))
                    .map(|level| u16::from(power.level_cost(level)))
                    .sum();
                assert_eq!(used_points(&levels, free_saber), charged, "{power:?}");
            }
        }
    }

    /// `side` and `is_team_power` must agree with what legalization strips.
    #[test]
    fn power_side_matches_legalization() {
        let team = ForceLegalizeRules {
            gametype: 6,
            ..ForceLegalizeRules::default()
        };
        for (index, power) in ForcePower::ALL.into_iter().enumerate() {
            // Offense stays at one: without it defense and throw are cleared.
            let mut digits = [b'0'; FORCE_POWER_COUNT];
            digits[SABER_OFFENSE] = b'1';
            digits[index] = b'1';
            let digits = std::str::from_utf8(&digits).unwrap();
            for side in [ForceSide::Light, ForceSide::Dark] {
                let input = format!("7-{}-{digits}", side as u8);
                let kept = legalize_force_powers(&input, team).allocation.levels[index] > 0;
                let expected = power.side().is_none_or(|power_side| power_side == side);
                assert_eq!(kept, expected, "{power:?} on {side:?}");
                let ffa = legalize_force_powers(&input, ForceLegalizeRules::default());
                let kept = ffa.allocation.levels[index] > 0;
                assert_eq!(kept, expected && !power.is_team_power(), "{power:?} in FFA");
            }
        }
    }
}
