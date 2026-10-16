//! Retail's "Team Color" chooser over the model grid (`UI_SKIN_COLOR`,
//! `codemp/ui/ui_main.c`): the grid lists the ordinary skins of one team
//! (`default`, `red` or `blue`) plus every species, and switching teams
//! keeps the model, swapping in its skin for the new team when it has one.
//! Skins retail never listed (siege, boss, ...) are part of Default here
//! (owner request), so nothing the catalogue found is unreachable.
//!
//! SJK adds a search over the grid: only models whose `model/skin` name (a
//! species' model name) contains every typed word are listed.

use super::controller::wrap;
use super::*;
use winit::keyboard::KeyCode;

/// Which skin set the grid lists.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum TeamSkin {
    #[default]
    Default,
    Red,
    Blue,
}

impl TeamSkin {
    pub(super) const ALL: [Self; 3] = [Self::Default, Self::Red, Self::Blue];

    /// The set a catalogue skin belongs to; anything but a team skin is
    /// Default.
    fn of(skin: &str) -> Self {
        match skin {
            "red" => Self::Red,
            "blue" => Self::Blue,
            _ => Self::Default,
        }
    }

    /// Skin name a model needs for this set.
    fn skin(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Red => "red",
            Self::Blue => "blue",
        }
    }

    pub(super) fn index(self) -> usize {
        Self::ALL.iter().position(|team| *team == self).unwrap_or(0)
    }
}

impl PlayerMenu {
    /// Refill the grid's tile list for the current team and search from the
    /// catalogue: matching characters first (catalogue order), then the
    /// matching species.
    pub(super) fn rebuild_tiles(&mut self) {
        self.tiles.clear();
        self.retail_tiles = 0;
        self.rig_tiles = 0;
        let Some(catalog) = catalog_of(&self.loader) else {
            return;
        };
        let team = self.team;
        let search = self.search.as_str();
        let characters = catalog.characters.iter().enumerate().filter(|(_, entry)| {
            TeamSkin::of(&entry.skin) == team && search_matches(search, &entry.cvar_value)
        });
        let species = catalog
            .species
            .iter()
            .enumerate()
            .filter(|(_, species)| search_matches(search, &species.model))
            .map(|(index, _)| (catalog.characters.len() + index, index));
        // The retail game's own first (its characters, then its species), then the
        // community's in the same order: the catalogue lists each retail first.
        let retail = |index: usize, count: usize| index < count;
        self.tiles.extend(
            characters
                .clone()
                .filter(|(index, _)| retail(*index, catalog.retail_characters))
                .map(|(index, _)| index),
        );
        self.tiles.extend(
            species
                .clone()
                .filter(|(_, index)| retail(*index, catalog.retail_species))
                .map(|(absolute, _)| absolute),
        );
        self.retail_tiles = self.tiles.len();
        // The high quality models (SJK rig models) next, then the rest of the
        // community's.
        let community = characters.filter(|(index, _)| !retail(*index, catalog.retail_characters));
        self.tiles.extend(
            community
                .clone()
                .filter(|(_, entry)| entry.rig)
                .map(|(index, _)| index),
        );
        self.rig_tiles = self.tiles.len() - self.retail_tiles;
        self.tiles.extend(
            community
                .filter(|(_, entry)| !entry.rig)
                .map(|(index, _)| index),
        );
        self.tiles.extend(
            species
                .filter(|(_, index)| !retail(*index, catalog.retail_species))
                .map(|(absolute, _)| absolute),
        );
    }

    /// Change the search to `text` and list what matches it from the top
    /// (or around the current model, when it matches).
    pub(super) fn set_search(&mut self, text: &str) {
        if self.search == text {
            return;
        }
        self.search.clear();
        self.search.push_str(text);
        self.rebuild_tiles();
        self.grid_scroll = 0;
        self.grid_follow = true;
        self.creator.back_to_top();
    }

    /// Start typing the search.
    pub(super) fn begin_search(&mut self) {
        self.name_editing = false;
        self.search_editing = true;
    }

    /// A key while the search is typed: text filters as it is typed,
    /// Backspace deletes, Enter keeps the search, Escape clears it.
    pub(super) fn edit_search(&mut self, event: &winit::event::KeyEvent, key: KeyCode) {
        let mut text = self.search.clone();
        self.search_editing = type_search(&mut text, event, key);
        self.set_search(&text);
    }

    /// A key while the Saber page's hilt search is typed, as [`Self::edit_search`].
    pub(super) fn edit_hilt_search(&mut self, event: &winit::event::KeyEvent, key: KeyCode) {
        let mut text = self.saber.search().to_owned();
        self.search_editing = type_search(&mut text, event, key);
        self.saber.set_search(&text);
    }
}

/// A key typed into a search field holding `text`: Escape clears it,
/// Backspace deletes, Enter keeps it, other keys add their text (up to
/// [`MAX_SEARCH`] characters). Returns whether typing goes on.
fn type_search(text: &mut String, event: &winit::event::KeyEvent, key: KeyCode) -> bool {
    match key {
        KeyCode::Escape => {
            text.clear();
            return false;
        }
        KeyCode::Enter | KeyCode::NumpadEnter => return false,
        KeyCode::Backspace => {
            text.pop();
        }
        _ => {
            if let Some(typed) = event.text.as_deref() {
                let room = MAX_SEARCH.saturating_sub(text.chars().count());
                text.extend(
                    typed
                        .chars()
                        .filter(|character| !character.is_control())
                        .take(room),
                );
            }
        }
    }
    true
}

/// Longest search, in characters (what the classic field shows).
const MAX_SEARCH: usize = 24;

/// Whether `name` contains every word of `search`, ignoring case (an empty
/// search matches everything).
pub(super) fn search_matches(search: &str, name: &str) -> bool {
    let name = name.to_lowercase();
    search
        .split_whitespace()
        .all(|word| name.contains(&word.to_lowercase()))
}

impl PlayerMenu {
    /// Team set of the current choice: the character's skin, or the team
    /// already shown for a species (which belongs to every set).
    pub(super) fn team_of_choice(&self) -> TeamSkin {
        match (self.choice, self.catalog()) {
            (Some(Choice::Character(index)), Some(catalog)) => catalog
                .characters
                .get(index)
                .map_or(self.team, |entry| TeamSkin::of(&entry.skin)),
            _ => self.team,
        }
    }

    /// Whether any character of the catalogue belongs to `team`.
    fn team_has_characters(&self, team: TeamSkin) -> bool {
        self.catalog().is_some_and(|catalog| {
            catalog
                .characters
                .iter()
                .any(|entry| TeamSkin::of(&entry.skin) == team)
        })
    }

    /// Step the team chooser to the next set that has characters, moving
    /// the current model onto its skin for that set when it has one.
    pub(super) fn cycle_team(&mut self, direction: isize) {
        let mut index = self.team.index();
        for _ in 1..TeamSkin::ALL.len() {
            index = wrap(index, direction, TeamSkin::ALL.len());
            if self.team_has_characters(TeamSkin::ALL[index]) {
                break;
            }
        }
        self.team = TeamSkin::ALL[index];
        self.rebuild_tiles();
        self.grid_follow = true;
        let Some(Choice::Character(current)) = self.choice else {
            return;
        };
        let skin = self.team.skin();
        let matching = self.catalog().and_then(|catalog| {
            let model = &catalog.characters.get(current)?.model;
            catalog
                .characters
                .iter()
                .position(|entry| entry.model == *model && entry.skin == skin)
        });
        if let Some(absolute) = matching {
            self.select_choice(absolute);
        }
    }

    /// Slot of the current choice in the grid, if the grid lists it.
    pub(super) fn tile_position(&self) -> Option<usize> {
        let current = self.choice_index();
        self.tiles.iter().position(|&absolute| absolute == current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A menu whose catalogue lists kyle (default, red, blue), jan and
    /// desann (default, dark).
    fn menu() -> PlayerMenu {
        menu_with(Vec::new())
    }

    /// [`menu`] with `community` mounted as a pack of the player's own.
    fn menu_with(community: Vec<(String, Vec<u8>)>) -> PlayerMenu {
        let mut files = Vec::new();
        for (model, skins) in [
            ("kyle", &["default", "red", "blue"][..]),
            ("jan", &["default"][..]),
            ("desann", &["default", "dark"][..]),
        ] {
            files.push((format!("models/players/{model}/model.glm"), b"x".to_vec()));
            for skin in skins {
                files.push((
                    format!("models/players/{model}/model_{skin}.skin"),
                    b"x".to_vec(),
                ));
                files.push((
                    format!("models/players/{model}/icon_{skin}.jpg"),
                    b"x".to_vec(),
                ));
            }
        }
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory("assets1", files).unwrap();
        if !community.is_empty() {
            vfs.mount_memory("community", community).unwrap();
        }
        let mut menu = PlayerMenu::new();
        menu.attach_catalogue(Arc::new(vfs));
        let loader = menu.loader.as_mut().unwrap();
        loader.request();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while loader.catalog().is_none() && std::time::Instant::now() < deadline {
            loader.poll();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        menu.rebuild_tiles();
        menu
    }

    fn listed(menu: &PlayerMenu) -> Vec<&str> {
        let catalog = catalog_of(&menu.loader).unwrap();
        menu.tiles
            .iter()
            .map(|&absolute| super::super::grid::entry_name(catalog, absolute).unwrap())
            .collect()
    }

    #[test]
    fn the_search_lists_the_models_containing_every_word() {
        let mut menu = menu();
        assert_eq!(
            listed(&menu),
            [
                "desann/dark",
                "desann/default",
                "jan/default",
                "kyle/default"
            ]
        );
        menu.set_search("KYLE");
        assert_eq!(listed(&menu), ["kyle/default"]);
        menu.set_search("desann dark");
        assert_eq!(listed(&menu), ["desann/dark"]);
        menu.set_search("  ");
        assert_eq!(listed(&menu).len(), 4);
        menu.set_search("zzz");
        assert!(listed(&menu).is_empty());
        // The search keeps to the team's skins.
        menu.set_search("kyle");
        menu.cycle_team(1);
        assert_eq!(listed(&menu), ["kyle/red"]);
    }

    /// A pack holding a community model and a rig model.
    fn with_rig() -> PlayerMenu {
        let mut files = Vec::new();
        for file in ["model.glm", "model_default.skin", "icon_default.jpg"] {
            files.push((format!("models/players/reborn2/{file}"), b"x".to_vec()));
        }
        for file in ["sjk_rig.cfg", "model.gltf", "icon_default.png"] {
            files.push((format!("models/players/wolfie/{file}"), b"x".to_vec()));
        }
        menu_with(files)
    }

    #[test]
    fn high_quality_models_come_between_the_games_and_the_communitys() {
        let menu = with_rig();
        let listed = listed(&menu);
        let wolfie = listed.iter().position(|name| *name == "wolfie/default");
        let reborn = listed.iter().position(|name| *name == "reborn2/default");
        assert_eq!(wolfie, Some(menu.retail_tiles), "{listed:?}");
        assert!(reborn > wolfie, "{listed:?}");
        assert_eq!(menu.rig_tiles, 1);
        // Without a rig model there is no such section.
        assert_eq!(super::tests::menu().rig_tiles, 0);
    }

    #[test]
    fn a_model_without_a_skin_is_its_default_and_an_unknown_one_opens_on_default() {
        let mut menu = with_rig();
        let worn = |menu: &mut PlayerMenu, model: &str| {
            menu.draft.model = model.to_owned();
            menu.resolved_catalogue = false;
            menu.resolve_current_choice();
            let catalog = catalog_of(&menu.loader).unwrap();
            let name = super::super::grid::entry_name(catalog, menu.choice_index()).unwrap();
            (name.to_owned(), menu.team)
        };
        assert_eq!(
            worn(&mut menu, "wolfie"),
            ("wolfie/default".to_owned(), TeamSkin::Default)
        );
        assert_eq!(
            worn(&mut menu, "kyle/blue"),
            ("kyle/blue".to_owned(), TeamSkin::Blue)
        );
        let (_, team) = worn(&mut menu, "nobody");
        assert_eq!(team, TeamSkin::Default);
    }

    #[test]
    fn words_match_anywhere_ignoring_case() {
        assert!(search_matches("", "kyle/default"));
        assert!(search_matches("DEF ky", "kyle/default"));
        assert!(!search_matches("kyle red", "kyle/default"));
        assert!(search_matches("ünï", "Ünïcode/x"));
    }
}
