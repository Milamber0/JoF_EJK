//! The SJK UI Saber page's blade choice (`docs/sjk-ui.md`, Character): the stock
//! blade in its colour, then every blade skin the player's hub profile lists, the
//! rarest tier first (as the Collection lists them). Locked skins are not offered: the Collection shows them. Picking
//! one sets `cg_saberSkin` as the Collection's Equip does ([`unlockables::wear`]),
//! by the same rules ([`Holdings::can_wear`], [`Holdings::worn_blade_skin`]), and the
//! stage model wears it once the looks read it again (twice a second).
//!
//! What the player owns is read from the identity service twice a second, and at
//! once when the screen opens or a choice is made.

use crate::console::ViewerConsole;
use crate::unlockables::{self, Holdings, SABER_SKIN_CVAR, Unlockable};
use std::sync::Arc;

/// How long a reading of the identity holds, in menu clock seconds.
const READ_EVERY: f64 = 0.5;

/// What the blade choice offers and which it shows chosen.
pub(super) struct BladeChoice {
    /// The blade skins the player owns, the rarest tier first.
    owned: Vec<&'static Unlockable>,
    /// The one worn; `None` for the stock blade.
    worn: Option<&'static Unlockable>,
    /// Why only the stock blade is offered when no skin is, in a few words.
    hint: &'static str,
    /// When the identity is read again (menu clock seconds).
    next_read: f64,
    /// The first choice the row shows while the player browses it (the wheel, the
    /// arrows); `None` keeps the chosen one in view.
    browsed: Option<usize>,
    /// The blade skins the hub's packs brought, the swatches' looks.
    skins: Arc<crate::saber_skins::LoadedSkins>,
    /// What a world shot or test shows in place of the own profile's unlocks.
    #[cfg(test)]
    pub(super) preview: Option<Vec<sjk_identity::Unlock>>,
}

impl BladeChoice {
    pub(super) fn new() -> Self {
        Self {
            owned: Vec::with_capacity(unlockables::ALL.len()),
            worn: None,
            hint: hint(&Holdings::IdentityOff),
            next_read: 0.0,
            browsed: None,
            skins: Default::default(),
            #[cfg(test)]
            preview: None,
        }
    }

    /// Read what the player owns and wears when the last reading is old.
    pub(super) fn follow(&mut self, console: &ViewerConsole) {
        let now = crate::menu::art::motion::seconds();
        if now >= self.next_read {
            self.next_read = now + READ_EVERY;
            self.read(console);
        }
        if self.skins.generation() != crate::sjk_packs::generation() {
            self.skins = crate::sjk_packs::skins();
        }
    }

    /// Read what the player owns and wears now.
    pub(super) fn read(&mut self, console: &ViewerConsole) {
        let setting = console.text_value(SABER_SKIN_CVAR).unwrap_or_default();
        #[cfg(test)]
        if let Some(unlocks) = &self.preview {
            let holdings = Holdings::Known(unlocks);
            Self::take(
                &mut self.owned,
                &mut self.worn,
                &mut self.hint,
                &holdings,
                setting,
            );
            return;
        }
        let enabled = console.bool_cvar("cl_identity") == Some(true);
        let wanted = crate::debug_collectibles::wanted(console);
        let read = crate::player_identity::with_snapshot(|snapshot| {
            // Copied only for the debug mode, which dresses the copy.
            let dressed = crate::debug_collectibles::active(wanted, snapshot)
                .then(|| crate::debug_collectibles::dressed(wanted, snapshot.clone()));
            let holdings = Holdings::of(enabled, Some(dressed.as_ref().unwrap_or(snapshot)));
            Self::take(
                &mut self.owned,
                &mut self.worn,
                &mut self.hint,
                &holdings,
                setting,
            );
        });
        if read.is_none() {
            Self::take(
                &mut self.owned,
                &mut self.worn,
                &mut self.hint,
                &Holdings::IdentityOff,
                setting,
            );
        }
    }

    /// Keep what `holdings` and `setting` say.
    fn take(
        owned: &mut Vec<&'static Unlockable>,
        worn: &mut Option<&'static Unlockable>,
        hint: &mut &'static str,
        holdings: &Holdings<'_>,
        setting: &str,
    ) {
        owned.clear();
        owned.extend(
            unlockables::blade_skins_by_tier()
                .iter()
                .copied()
                .filter(|unlockable| holdings.can_wear(unlockable)),
        );
        *worn = holdings.worn_blade_skin(setting);
        *hint = self::hint(holdings);
    }

    /// How many choices there are: the stock blade and the skins owned.
    pub(super) fn count(&self) -> usize {
        1 + self.owned.len()
    }

    /// Choice `index`: `None` the stock blade, else a blade skin; `None` past the end
    /// (the outer option).
    pub(super) fn get(&self, index: usize) -> Option<Option<&'static Unlockable>> {
        match index {
            0 => Some(None),
            _ => self.owned.get(index - 1).copied().map(Some),
        }
    }

    /// The choice shown chosen: the skin worn, else the stock blade.
    pub(super) fn chosen(&self) -> usize {
        self.worn
            .and_then(|worn| self.owned.iter().position(|skin| skin.id == worn.id))
            .map_or(0, |at| at + 1)
    }

    /// The blade skin worn, `None` for the stock blade.
    pub(super) fn worn(&self) -> Option<&'static Unlockable> {
        self.worn
    }

    /// Why only the stock blade is offered: the identity is off, no hub is set or
    /// answered yet, or no skin is owned. `None` while skins are offered.
    pub(super) fn hint(&self) -> Option<&'static str> {
        self.owned.is_empty().then_some(self.hint)
    }

    /// The loaded look of blade skin `id`, when its pack has come.
    pub(super) fn look(&self, id: &str) -> Option<&crate::saber_skins::LoadedSkin> {
        self.skins.get(id)
    }

    /// Draw the swatches with `skins` (tests and world shots, in place of the packs').
    #[cfg(test)]
    pub(super) fn set_looks(&mut self, skins: Arc<crate::saber_skins::LoadedSkins>) {
        self.skins = skins;
    }

    /// Make choice `index` the blade and write it, unless it is the one shown chosen
    /// (so a skin chosen before the hub answered is not dropped by a click on the
    /// stock blade that already shows).
    pub(super) fn pick(&mut self, console: &mut ViewerConsole, index: usize) {
        let Some(choice) = self.get(index) else {
            return;
        };
        self.browsed = None;
        if index == self.chosen() {
            return;
        }
        unlockables::wear(console, choice.map_or("", |skin| skin.id));
        self.worn = choice;
    }

    /// The first choice of a row that shows `shown` at once: where the player browsed
    /// to, else `following` (the window that keeps the chosen one in view).
    pub(super) fn first(&self, following: usize, shown: usize) -> usize {
        self.browsed
            .unwrap_or(following)
            .min(self.count().saturating_sub(shown))
    }

    /// Browse the row `by` choices (negative to the left) from `first`, without wearing
    /// anything; the next pick or step follows the chosen one again.
    pub(super) fn browse(&mut self, first: usize, by: isize, shown: usize) {
        let last = self.count().saturating_sub(shown);
        self.browsed = Some(first.saturating_add_signed(by).min(last));
    }

    /// Move the choice `direction` along, wrapping, and write it.
    pub(super) fn step(&mut self, console: &mut ViewerConsole, direction: isize) {
        let next = super::controller::wrap(self.chosen(), direction, self.count());
        self.pick(console, next);
        self.browsed = None;
    }
}

/// Why `holdings` offer no blade skin, in a few words.
fn hint(holdings: &Holdings<'_>) -> &'static str {
    match holdings {
        Holdings::IdentityOff => "Shaders need the SJK identity",
        Holdings::NoHub => "Shaders need the SJK hub",
        Holdings::Waiting => "Waiting for the SJK hub...",
        Holdings::Known(_) => "No saber shaders yet: see Collection",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unlock(id: &str) -> sjk_identity::Unlock {
        sjk_identity::Unlock {
            id: id.to_owned(),
            granted: 1_791_336_225,
            note: String::new(),
            count: 1,
            medal: None,
        }
    }

    fn console() -> (tempfile::TempDir, ViewerConsole) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        (directory, console)
    }

    /// The stock blade, then the owned skins, the rarest tier first; the locked ones and
    /// unknown ids are left out, and picking writes `cg_saberSkin` as Equip does.
    #[test]
    fn the_stock_blade_and_the_owned_skins_are_offered_and_picking_wears_one() {
        let (_directory, mut console) = console();
        let mut choice = BladeChoice::new();
        choice.preview = Some(vec![
            unlock("saber_void"),
            unlock("saber_moon"),
            unlock("saber_sun"),
        ]);
        choice.read(&console);
        assert_eq!(choice.count(), 3);
        assert_eq!(
            choice.get(1).flatten().map(|skin| skin.id),
            Some("saber_sun")
        );
        assert_eq!(
            choice.get(2).flatten().map(|skin| skin.id),
            Some("saber_void")
        );
        assert_eq!(choice.get(3), None);
        assert_eq!(choice.chosen(), 0);
        assert_eq!(choice.hint(), None);
        choice.pick(&mut console, 2);
        assert_eq!(console.text_value(SABER_SKIN_CVAR), Some("saber_void"));
        assert_eq!(choice.chosen(), 2);
        choice.read(&console);
        assert_eq!(choice.chosen(), 2, "read back from the setting");
        choice.step(&mut console, 1);
        assert_eq!(console.text_value(SABER_SKIN_CVAR), Some(""));
        choice.step(&mut console, -1);
        assert_eq!(console.text_value(SABER_SKIN_CVAR), Some("saber_void"));
    }

    /// The row is browsed without wearing anything, never past either end, and picking
    /// a blade follows the chosen one again.
    #[test]
    fn the_row_is_browsed_without_wearing_and_a_pick_follows_again() {
        let (_directory, mut console) = console();
        let mut choice = BladeChoice::new();
        choice.preview = Some(
            unlockables::blade_skins()
                .map(|skin| unlock(skin.id))
                .collect(),
        );
        choice.read(&console);
        let count = choice.count();
        assert!(count > 12);
        assert_eq!(choice.first(0, 6), 0);
        choice.browse(0, 6, 6);
        assert_eq!(choice.first(0, 6), 6);
        assert_eq!(
            console.text_value(SABER_SKIN_CVAR),
            Some(""),
            "nothing worn"
        );
        choice.browse(6, 1_000, 6);
        assert_eq!(choice.first(0, 6), count - 6, "stops at the end");
        choice.browse(count - 6, -1_000, 6);
        assert_eq!(choice.first(3, 6), 0, "and at the start");
        choice.pick(&mut console, 9);
        assert_eq!(choice.first(7, 6), 7, "follows the chosen one again");
    }

    /// With nothing owned only the stock blade shows, with a line saying why, and a
    /// skin chosen but not (yet) owned stays chosen until another blade is picked.
    #[test]
    fn without_skins_only_the_stock_blade_shows_and_a_pending_choice_stays() {
        let (_directory, mut console) = console();
        console.set_cvar(SABER_SKIN_CVAR, "saber_sun");
        let mut choice = BladeChoice::new();
        choice.preview = Some(Vec::new());
        choice.read(&console);
        assert_eq!(choice.count(), 1);
        assert_eq!(choice.worn(), None);
        assert_eq!(choice.hint(), Some("No saber shaders yet: see Collection"));
        choice.pick(&mut console, 0);
        choice.step(&mut console, 1);
        assert_eq!(console.text_value(SABER_SKIN_CVAR), Some("saber_sun"));
        // The live identity: none in tests, so it is off.
        choice.preview = None;
        choice.read(&console);
        assert_eq!(choice.hint(), Some("Shaders need the SJK identity"));
    }
}
