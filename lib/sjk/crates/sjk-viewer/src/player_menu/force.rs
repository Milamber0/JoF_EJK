//! Legal Force-profile editing state backed solely by `ForceAllocation`.
//!
//! The page edits a draft. Unlike the rest of the player screen, nothing is
//! written while the player picks: Apply writes `forcepowers` once and Discard
//! returns the draft to what was last applied. Leaving the page, by another
//! page or by closing the screen, applies as retail's Force menu did on close
//! ([`ForceMenu::leave`]): the draft when it changed, else the profile again,
//! so the server is told to read it (`forcechanged`) either way.
//!
//! On a server the draft takes the server's rank ceiling and free saber
//! skills, which decide whether it keeps the profile it is sent
//! (`WP_InitForcePowers` parks a player whose profile it had to cut). What the
//! server merely will not let be used, its `g_forcePowerDisable` powers and
//! team powers outside team games, stays buyable and is marked
//! ([`ForceMenu::server_limit`]): the profile keeps them for a mod's full
//! Force duel or the next server.

use crate::console::ViewerConsole;
use sjk_client::{
    ForceAllocation, ForceLegalizeRules, ForcePower, ForceSide, legalize_force_powers,
};

pub(super) const POWER_NAMES: [&str; 18] = [
    "Heal",
    "Jump",
    "Speed",
    "Push",
    "Pull",
    "Mind Trick",
    "Grip",
    "Lightning",
    "Rage",
    "Protect",
    "Absorb",
    "Team Heal",
    "Team Energize",
    "Drain",
    "Sense",
    "Saber Offense",
    "Saber Defense",
    "Saber Throw",
];

/// What each power does, in a line or two, in `POWER_NAMES` order: the SJK
/// UI's Force page shows it for the power under the pointer.
pub(super) const POWER_NOTES: [&str; 18] = [
    "Restores your health. Each level heals more.",
    "Leaps higher and farther. Each level reaches higher ledges.",
    "Moves you in a blur for a few seconds, faster at each level.",
    "Throws players, sabers and objects away from you; stronger at each level.",
    "Draws players, sabers and objects towards you.",
    "Hides you from the players you trick; higher levels trick more of them, for longer.",
    "Holds an enemy in the air and chokes them; higher levels hold them longer.",
    "Strikes with Force lightning; higher levels reach farther and wider.",
    "Fights faster and harder for a while, draining your health. Tires you after.",
    "Shields your health with your Force, taking damage from the Force pool instead.",
    "Draws in the Force powers used on you to refill your own Force.",
    "Heals the teammates around you. Team games only.",
    "Refills the Force of the teammates around you. Team games only.",
    "Drains health from an enemy into yourself.",
    "Shows enemies through walls and those hiding with Mind Trick.",
    "Your saber stances: each level opens one more (blue, yellow, then red).",
    "Blocks more blows and shots with your saber, from more angles.",
    "Throws your saber and calls it back; farther and longer at each level.",
];

/// Stock `forcepowers` default, used when the cvar is unset.
const DEFAULT_FORCEPOWERS: &str = "7-1-032330000000001333";

/// `GT_TEAM` (`bg_public.h`): team powers are legal from here on.
pub(super) const GT_TEAM: i32 = 6;

/// Why the server the client plays on will not let a power be used as the
/// draft holds it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ServerLimit {
    /// `g_forcePowerDisable` turns it off.
    Disabled,
    /// `g_forcePowerDisable` holds it at this level (Jump at 1, Saber Offense
    /// and Defense at 3, as `BG_LegalizedForcePowers` sets them).
    Fixed(u8),
    /// A team power on a server that is not playing a team game.
    TeamGamesOnly,
}

impl ServerLimit {
    /// The few words a power's row says of it.
    pub(super) fn tag(self) -> &'static str {
        match self {
            Self::Disabled => "Off on this server",
            Self::Fixed(_) => "Fixed on this server",
            Self::TeamGamesOnly => "Team games only",
        }
    }

    /// What the page says of it.
    pub(super) fn describe(self) -> &'static str {
        match self {
            Self::Disabled => "Disabled on this server",
            Self::Fixed(1) => "Held at level 1 on this server",
            Self::Fixed(_) => "Held at level 3 on this server",
            Self::TeamGamesOnly => "Not used here: team games only",
        }
    }
}

/// What buying the next level of a power would take.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NextLevel {
    /// The power is at level 3.
    Mastered,
    /// The next level comes without points.
    Free,
    /// The next level costs this many points and the draft has them.
    Costs(u8),
    /// The next level costs this many points, more than are left.
    Short(u8),
    /// The other side's power.
    OtherSide,
    /// Saber Defense or Throw without Saber Offense, which legalization
    /// clears (`UI_ForcePowerRank_HandleKey`).
    NeedsOffense,
}

pub(super) struct ForceMenu {
    allocation: ForceAllocation,
    /// The profile `forcepowers` holds: read on open, replaced on Apply.
    applied: ForceAllocation,
    /// What the draft is kept legal under.
    rules: ForceLegalizeRules,
    /// The rules of the server the client plays on, read on open.
    server: Option<ForceLegalizeRules>,
    encoded: String,
    /// The template the draft was last loaded from, until it is edited.
    template: Option<String>,
    /// Apply already sent the profile during this visit of the page.
    sent_this_visit: bool,
}

impl ForceMenu {
    pub(super) fn new() -> Self {
        let allocation = ForceAllocation::default();
        Self {
            encoded: allocation.encode(),
            applied: allocation.clone(),
            allocation,
            rules: ForceLegalizeRules::default(),
            server: None,
            template: None,
            sent_this_visit: false,
        }
    }

    pub(super) fn open(&mut self, console: &ViewerConsole) {
        let raw = console
            .text_value("forcepowers")
            .unwrap_or(DEFAULT_FORCEPOWERS);
        self.load(raw, console.server_force_rules());
    }

    /// Start a fresh draft from `raw` as on a server with `rules`, for tests.
    #[cfg(test)]
    pub(super) fn load_on_server(&mut self, raw: &str, rules: ForceLegalizeRules) {
        self.load(raw, Some(rules));
    }

    /// Start a fresh draft from `raw`, at the server's rank and with its free
    /// saber skills on one (`server`), at the profile's own rank off one.
    fn load(&mut self, raw: &str, server: Option<ForceLegalizeRules>) {
        let mut allocation = ForceAllocation::parse(raw).unwrap_or_default();
        if let Some(server) = server {
            allocation.rank = server.max_rank.min(7);
        }
        self.server = server;
        self.rules = ForceLegalizeRules {
            max_rank: allocation.rank,
            free_saber: server.is_some_and(|server| server.free_saber),
            // Kept for what a server only marks: team powers, disabled ones.
            gametype: GT_TEAM,
            team_side: None,
            disabled_mask: 0,
        };
        self.allocation = legalize_force_powers(&allocation.encode(), self.rules).allocation;
        // What was read counts as applied: opening never writes the cvar.
        self.applied.clone_from(&self.allocation);
        self.template = None;
        self.refresh_encoded();
    }

    /// Load a template's `forcepowers` into the draft, legalized under the
    /// server's rules at the draft's rank (`UI_ForceConfigHandle`).
    pub(super) fn load_template(&mut self, name: &str, value: &str) {
        self.allocation = legalize_force_powers(value, self.rules).allocation;
        self.template = Some(name.to_owned());
        self.refresh_encoded();
    }

    /// The template the draft holds unedited, if any.
    pub(super) fn template(&self) -> Option<&str> {
        self.template.as_deref()
    }

    pub(super) fn allocation(&self) -> &ForceAllocation {
        &self.allocation
    }

    /// Points remaining under the same free-saber policy as the editor's spend/refund path.
    pub(super) fn remaining_points(&self) -> u16 {
        self.allocation.remaining_points(self.rules.free_saber)
    }

    /// Whether the draft differs from the applied profile.
    pub(super) fn is_dirty(&self) -> bool {
        self.allocation != self.applied
    }

    /// Points the draft's rank grants in all.
    pub(super) fn total_points(&self) -> u16 {
        sjk_client::mastery_points(self.allocation.rank)
    }

    /// Whether Saber Offense and Defense start at level 1 for free.
    pub(super) fn free_saber(&self) -> bool {
        self.rules.free_saber
    }

    /// The rules of the server the client plays on, when it is on one.
    pub(super) fn server(&self) -> Option<ForceLegalizeRules> {
        self.server
    }

    /// Why the server will not let power `index` be used as the draft has it,
    /// if it will not.
    pub(super) fn server_limit(&self, index: usize) -> Option<ServerLimit> {
        let server = self.server?;
        let power = ForcePower::ALL.get(index).copied()?;
        if server.disabled_mask & (1 << index) != 0 {
            return Some(match power {
                ForcePower::Levitation => ServerLimit::Fixed(1),
                ForcePower::SaberOffense | ForcePower::SaberDefense => ServerLimit::Fixed(3),
                _ => ServerLimit::Disabled,
            });
        }
        (power.is_team_power() && server.gametype < GT_TEAM).then_some(ServerLimit::TeamGamesOnly)
    }

    /// What raising power `index` one level would take.
    pub(super) fn next_level(&self, index: usize) -> NextLevel {
        let Some(power) = ForcePower::ALL.get(index).copied() else {
            return NextLevel::Mastered;
        };
        if power
            .side()
            .is_some_and(|side| side != self.allocation.side)
        {
            return NextLevel::OtherSide;
        }
        if matches!(power, ForcePower::SaberDefense | ForcePower::SaberThrow)
            && self.allocation.levels[ForcePower::SaberOffense as usize] == 0
        {
            return NextLevel::NeedsOffense;
        }
        let level = self.allocation.levels[index];
        if level >= 3 {
            return NextLevel::Mastered;
        }
        if power.level_is_free(level + 1, self.rules.free_saber) {
            return NextLevel::Free;
        }
        let cost = power.level_cost(level + 1);
        if u16::from(cost) > self.remaining_points() {
            NextLevel::Short(cost)
        } else {
            NextLevel::Costs(cost)
        }
    }

    /// Points that raising power `index` from its level to `level` would
    /// cost (0 when `level` is not above it).
    pub(super) fn cost_to(&self, index: usize, level: u8) -> u16 {
        let Some(power) = ForcePower::ALL.get(index).copied() else {
            return 0;
        };
        let current = self.allocation.levels[index];
        (current + 1..=level.min(3))
            .filter(|level| !power.level_is_free(*level, self.rules.free_saber))
            .map(|level| u16::from(power.level_cost(level)))
            .sum()
    }

    /// Move power `index` straight to `level`, one legal step at a time;
    /// when a step fails the draft is left as it was.
    pub(super) fn set_level(&mut self, index: usize, level: u8) -> bool {
        let Some(current) = self.allocation.levels.get(index).copied() else {
            return false;
        };
        let level = level.min(3);
        if level == current {
            return false;
        }
        let before = self.allocation.clone();
        let raise = level > current;
        for _ in 0..current.abs_diff(level) {
            if !self.step(index, raise) {
                self.allocation = before;
                self.refresh_encoded();
                return false;
            }
        }
        true
    }

    pub(super) fn set_side(&mut self, side: ForceSide) {
        if self.allocation.side == side {
            return;
        }
        self.template = None;
        self.allocation.side = side;
        self.allocation = legalize_force_powers(&self.allocation.encode(), self.rules).allocation;
        self.refresh_encoded();
    }

    pub(super) fn step(&mut self, index: usize, increase: bool) -> bool {
        let Some(power) = ForcePower::ALL.get(index).copied() else {
            return false;
        };
        let before = self.allocation.clone();
        let changed = if increase {
            self.allocation.spend(power, self.rules.free_saber)
        } else {
            self.allocation.refund(power, self.rules.free_saber)
        };
        if changed {
            let legalized = legalize_force_powers(&self.allocation.encode(), self.rules).allocation;
            if legalized != self.allocation {
                self.allocation = before;
                return false;
            }
            self.template = None;
            self.refresh_encoded();
        }
        changed
    }

    /// Clear every level on the draft (the free minima stay).
    pub(super) fn reset(&mut self) {
        self.template = None;
        let side = self.allocation.side;
        self.allocation = ForceAllocation {
            rank: self.rules.max_rank,
            side,
            levels: [0; 18],
        };
        self.allocation = legalize_force_powers(&self.allocation.encode(), self.rules).allocation;
        self.refresh_encoded();
    }

    /// Return the draft to the applied profile.
    pub(super) fn discard(&mut self) {
        self.template = None;
        self.allocation.clone_from(&self.applied);
        self.refresh_encoded();
    }

    /// Write the draft to `forcepowers` and, on a server, have the server read
    /// it; nothing happens when it is unchanged.
    pub(super) fn apply(&mut self, console: &mut ViewerConsole) {
        if let Some(value) = self.commit() {
            console.apply_forcepowers(value);
            self.sent_this_visit = true;
        }
    }

    /// The page came on show: a new visit, which [`Self::leave`] closes.
    pub(super) fn enter(&mut self) {
        self.sent_this_visit = false;
    }

    /// The page went off show: apply the draft when it changed, else send the
    /// profile `forcepowers` holds again, unless Apply already sent it during
    /// this visit. The unchanged profile is resent as the player wrote it, not
    /// as the draft fitted it to the server's rank.
    pub(super) fn leave(&mut self, console: &mut ViewerConsole) {
        if self.is_dirty() {
            self.apply(console);
        } else if !self.sent_this_visit {
            let current = console
                .text_value("forcepowers")
                .unwrap_or(DEFAULT_FORCEPOWERS)
                .to_owned();
            console.apply_forcepowers(&current);
        }
        self.sent_this_visit = false;
    }

    /// Mark the draft applied and return its `forcepowers` value, or `None`
    /// when there is nothing to apply.
    fn commit(&mut self) -> Option<&str> {
        if !self.is_dirty() {
            return None;
        }
        self.applied.clone_from(&self.allocation);
        Some(&self.encoded)
    }

    fn refresh_encoded(&mut self) {
        self.encoded = self.allocation.encode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu(raw: &str) -> ForceMenu {
        let mut menu = ForceMenu::new();
        menu.load(raw, None);
        menu
    }

    #[test]
    fn edits_stay_in_the_draft_until_applied() {
        let mut menu = menu(DEFAULT_FORCEPOWERS);
        assert!(!menu.is_dirty());
        assert!(menu.commit().is_none());
        assert!(menu.step(2, false)); // Speed 2 -> 1
        assert!(menu.is_dirty());
        assert_eq!(menu.commit(), Some("7-1-031330000000001333"));
        assert!(!menu.is_dirty());
        assert!(menu.commit().is_none());
    }

    #[test]
    fn discard_returns_to_the_applied_profile() {
        let mut menu = menu(DEFAULT_FORCEPOWERS);
        menu.set_side(ForceSide::Dark);
        menu.reset();
        assert!(menu.is_dirty());
        menu.discard();
        assert!(!menu.is_dirty());
        assert_eq!(menu.allocation().encode(), DEFAULT_FORCEPOWERS);
    }

    #[test]
    fn stepping_back_to_the_applied_profile_is_clean() {
        let mut menu = menu(DEFAULT_FORCEPOWERS);
        assert!(menu.step(2, false));
        assert!(menu.step(2, true));
        assert!(!menu.is_dirty());
    }

    #[test]
    fn opening_legalizes_without_counting_as_a_change() {
        // Grip on the light side is stripped on open, not left pending.
        let menu = menu("7-1-030000100000001000");
        assert!(!menu.is_dirty());
        assert_eq!(menu.allocation().levels[6], 0);
    }

    #[test]
    fn a_template_loads_legalized_at_the_drafts_rank() {
        // Rank 3 (20 points): retail's Knight is cut down to fit.
        let mut menu = menu("3-1-000000000000000000");
        menu.load_template("Knight", "7-1-331322000200003322");
        assert_eq!(menu.allocation().rank, 3);
        assert!(menu.remaining_points() <= 20);
        assert_eq!(menu.template(), Some("Knight"));
        assert!(menu.is_dirty());
        // Any edit leaves the template behind.
        menu.reset();
        assert_eq!(menu.template(), None);
        let mut menu = menu_at_master();
        menu.load_template("Destroyer", "7-2-012320333000030321");
        assert_eq!(menu.allocation().side, ForceSide::Dark);
        assert_eq!(menu.allocation().levels[7], 3); // Lightning
        menu.set_side(ForceSide::Light);
        assert_eq!(menu.template(), None);
    }

    fn menu_at_master() -> ForceMenu {
        menu(DEFAULT_FORCEPOWERS)
    }

    #[test]
    fn next_level_says_what_it_takes() {
        // Stock profile: Jump 3, Speed 2, Push and Pull 3, the saber 3s;
        // 82 of 100 points spent.
        let mut menu = menu(DEFAULT_FORCEPOWERS);
        assert_eq!(menu.next_level(3), NextLevel::Mastered); // Push 3
        assert_eq!(menu.next_level(6), NextLevel::OtherSide); // Grip, dark
        assert_eq!(menu.next_level(11), NextLevel::Costs(1)); // Team Heal
        assert_eq!(menu.server_limit(11), None, "off a server");
        assert_eq!(menu.next_level(2), NextLevel::Costs(6)); // Speed 2 -> 3
        assert_eq!(menu.cost_to(0, 3), 12); // Heal 0 -> 3: 2 + 4 + 6
        menu.reset();
        assert_eq!(menu.next_level(16), NextLevel::NeedsOffense);
        assert_eq!(menu.next_level(1), NextLevel::Costs(2)); // Jump 1 -> 2
        let poor = self::menu("1-1-000000000000000000"); // 5 points
        assert_eq!(poor.next_level(5), NextLevel::Costs(4)); // Mind Trick 1
        assert_eq!(poor.next_level(7), NextLevel::OtherSide);
        assert_eq!(poor.next_level(9), NextLevel::Costs(2));
        let mut broke = self::menu("1-1-000000000000000000");
        assert!(broke.set_level(5, 1)); // 4 of 5 spent
        assert_eq!(broke.next_level(9), NextLevel::Short(2));
    }

    #[test]
    fn set_level_moves_in_one_go_or_not_at_all() {
        let mut menu = menu("1-1-000000000000000000"); // 5 points
        assert!(!menu.set_level(0, 2)); // Heal 2 costs 2 + 4
        assert_eq!(menu.allocation().levels[0], 0);
        let mut menu = self::menu("2-1-000000000000000000"); // 10 points
        assert!(menu.set_level(0, 2)); // Heal 2 costs 6
        assert_eq!(menu.allocation().levels[0], 2);
        assert!(!menu.set_level(0, 3)); // 6 more needed, 4 left
        assert_eq!(menu.allocation().levels[0], 2);
        assert!(menu.set_level(0, 0));
        assert_eq!(menu.remaining_points(), 10);
        assert!(!menu.set_level(1, 0)); // Jump keeps its free level
        assert_eq!(menu.allocation().levels[1], 1);
    }

    /// A rank 4 (30 points) free-for-all with Heal (bit 0), Jump (1) and
    /// Grip (6) disabled and free saber skills.
    fn adept_server() -> ForceLegalizeRules {
        ForceLegalizeRules {
            max_rank: 4,
            free_saber: true,
            team_side: None,
            gametype: 0,
            disabled_mask: 0b100_0011,
        }
    }

    #[test]
    fn on_a_server_the_draft_takes_its_rank_and_marks_what_it_will_not_use() {
        let mut menu = ForceMenu::new();
        // Push 3 and the saber skills at 2: 20 of the 30 points.
        menu.load("7-1-010300000000000220", Some(adept_server()));
        assert_eq!(menu.allocation().rank, 4);
        assert_eq!(menu.total_points(), 30);
        assert!(menu.free_saber());
        assert_eq!(menu.remaining_points(), 10);
        assert!(!menu.is_dirty());
        assert_eq!(menu.server_limit(0), Some(ServerLimit::Disabled));
        assert_eq!(menu.server_limit(1), Some(ServerLimit::Fixed(1)));
        assert_eq!(menu.server_limit(6), Some(ServerLimit::Disabled));
        assert_eq!(menu.server_limit(11), Some(ServerLimit::TeamGamesOnly));
        assert_eq!(menu.server_limit(3), None);
        // A disabled power and a team power can still be bought, for the
        // profile's full Force duels or the next server.
        assert!(menu.set_level(3, 0)); // Push 3 -> 0 frees 10 points
        assert!(menu.set_level(0, 2)); // Heal 2: 6 points
        assert!(menu.step(11, true)); // Team Heal 1: 1 point
        assert_eq!(menu.allocation().levels[0], 2);
        assert_eq!(menu.allocation().levels[11], 1);
        assert_eq!(menu.remaining_points(), 13);
        assert_eq!(menu.commit(), Some("4-1-210000000001000220"));
    }

    fn console() -> (tempfile::TempDir, ViewerConsole) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        (directory, console)
    }

    /// Leaving the page applies a changed draft, as retail's Force menu did on close.
    #[test]
    fn leaving_the_page_applies_a_changed_draft() {
        let (_directory, mut console) = console();
        assert!(console.set_cvar("forcepowers", DEFAULT_FORCEPOWERS));
        let mut menu = ForceMenu::new();
        menu.open(&console);
        menu.enter();
        assert!(menu.step(2, false)); // Speed 2 -> 1
        menu.leave(&mut console);
        assert_eq!(
            console.text_value("forcepowers"),
            Some("7-1-031330000000001333")
        );
        assert!(!menu.is_dirty());
    }

    /// Leaving unchanged sends the player's own profile again, not the draft
    /// fitted to a lower server rank; after Apply in the same visit, nothing more.
    #[test]
    fn leaving_unchanged_resends_the_players_own_profile() {
        let (_directory, mut console) = console();
        let own = "7-1-330000000330000333";
        assert!(console.set_cvar("forcepowers", own));
        let mut menu = ForceMenu::new();
        menu.load_on_server(own, adept_server());
        menu.enter();
        menu.leave(&mut console);
        assert_eq!(console.text_value("forcepowers"), Some(own));
        menu.enter();
        menu.reset();
        assert!(menu.is_dirty());
        menu.apply(&mut console);
        assert!(menu.sent_this_visit);
        let applied = console.text_value("forcepowers").unwrap().to_owned();
        assert!(console.set_cvar("forcepowers", own));
        menu.leave(&mut console);
        assert_eq!(
            console.text_value("forcepowers"),
            Some(own),
            "Apply already sent this visit's profile ({applied})"
        );
        assert!(!menu.sent_this_visit);
    }
}
