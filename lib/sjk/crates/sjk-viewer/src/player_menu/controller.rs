//! Catalogue lifecycle, immediate cvar application (the Force page's
//! explicit Apply aside) and keyboard handling.

use super::rows::{
    CharacterRow, FORCE_APPLY_ROW, FORCE_DISCARD_ROW, FORCE_POWER_ROW, FORCE_RESET_ROW,
    FORCE_SIDE_ROW, SaberRow,
};
use super::*;
use sjk_client::{ForceSide, LegacyCatalogStatus, LegacySpecies};
use sjk_vfs::VirtualFileSystem;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

impl PlayerMenu {
    pub(crate) fn attach_catalogue(&mut self, vfs: Arc<VirtualFileSystem>) {
        self.loader = Some(LegacyAssetCatalogLoader::new(Arc::clone(&vfs)));
        self.icon_vfs = Some(vfs);
    }

    /// Read the model, sabers and cosmetics from the cvars for the Collection's
    /// stage, the screen staying closed.
    pub(crate) fn read_for_stage(&mut self, console: &ViewerConsole) {
        self.read_console(console);
        self.saber.open(console);
        self.cosmetics.open(console, self.icon_vfs.as_ref());
    }

    pub(crate) fn open(&mut self, console: &ViewerConsole, target: ReturnTarget) {
        self.return_target = target;
        self.hub = false;
        self.read_console(console);
        self.saber.open(console);
        self.force.open(console);
        self.cosmetics.open(console, self.icon_vfs.as_ref());
        self.numeric = None;
        self.name_editing = false;
        self.search_editing = false;
        self.search.clear();
        self.saber.set_search("");
        self.blade_choice.read(console);
        self.page = ProfilePage::Character;
        self.selected = 0;
        self.creator.reset();
        self.turn.reset();
        self.resolved_catalogue = false;
        if let Some(loader) = &mut self.loader {
            loader.request();
        }
        self.resolve_current_choice();
        self.reconcile_saber_style();
        self.grid_follow = true;
        self.request_icons_if_ready();
        if self.classic_style {
            self.show_classic(super::classic::ClassicPage::Player);
        }
    }

    /// Seed the drafts from the config at startup, so the stage model and
    /// its sabers are the player's own before this screen is ever opened,
    /// and start loading the catalogue those need resolving against.
    pub(crate) fn prime(&mut self, console: &ViewerConsole) {
        self.read_console(console);
        self.saber.open(console);
        if let Some(loader) = &mut self.loader {
            loader.request();
        }
    }

    /// Whether the catalogue has been matched against the drafts yet.
    pub(crate) fn is_resolved(&self) -> bool {
        self.resolved_catalogue
    }

    pub(crate) fn poll(&mut self) {
        if let Some(loader) = &mut self.loader {
            loader.poll();
        }
        self.resolve_current_choice();
        self.reconcile_saber_style();
        self.request_icons_if_ready();
        self.icons.poll();
        self.force_icons.poll();
        self.request_part_icons();
        self.part_icons.poll();
    }

    /// Character creation shows the species' part pictures: load them for
    /// the species being edited.
    fn request_part_icons(&mut self) {
        let classic_wants =
            self.classic_style && self.classic.page == super::classic::ClassicPage::Character;
        let sjk_wants = self.is_sjk()
            && self.page == ProfilePage::Character
            && self.category().part().is_some();
        if !classic_wants && !sjk_wants {
            return;
        }
        let Some(Choice::Species(index)) = self.choice else {
            return;
        };
        let (Some(vfs), Some(species)) = (
            self.icon_vfs.as_ref(),
            catalog_of(&self.loader).and_then(|catalog| catalog.species.get(index)),
        ) else {
            return;
        };
        self.part_icons.show(vfs, index, species);
    }

    pub(super) fn reconcile_saber_style(&mut self) {
        let catalog = catalog_of(&self.loader);
        self.saber.reconcile_style(catalog);
    }

    pub(super) fn status(&self) -> LegacyCatalogStatus {
        self.loader.as_ref().map_or(
            LegacyCatalogStatus::Failed,
            LegacyAssetCatalogLoader::status,
        )
    }

    fn read_console(&mut self, console: &ViewerConsole) {
        self.draft.name.clear();
        self.draft
            .name
            .push_str(console.text_value("name").unwrap_or("Padawan"));
        self.draft.model.clear();
        self.draft
            .model
            .push_str(console.text_value("model").unwrap_or("kyle/default"));
        self.draft.rgb = [
            cvar_u8(console, "char_color_red", 255),
            cvar_u8(console, "char_color_green", 255),
            cvar_u8(console, "char_color_blue", 255),
        ];
        self.applied_model.clone_from(&self.draft.model);
        self.applied_rgb = self.draft.rgb;
    }

    /// Write the character values to the console. Called after every change. The SJK
    /// UI keeps the model (and its team colour) in the draft, shown on the stage, until
    /// [`Self::apply_look`]; the name is written at once.
    pub(super) fn apply(&mut self, console: &mut ViewerConsole) {
        console.set_cvar("name", &self.draft.name);
        if !self.is_sjk() {
            self.apply_look(console);
        }
    }

    /// Write the draft's model and team colour to the console: the model is worn.
    pub(super) fn apply_look(&mut self, console: &mut ViewerConsole) {
        console.set_cvar("model", &self.draft.model);
        console.set_cvar("char_color_red", &self.draft.rgb[0].to_string());
        console.set_cvar("char_color_green", &self.draft.rgb[1].to_string());
        console.set_cvar("char_color_blue", &self.draft.rgb[2].to_string());
        self.applied_model.clone_from(&self.draft.model);
        self.applied_rgb = self.draft.rgb;
    }

    /// Whether the SJK UI shows a model that is not the one worn yet.
    pub(super) fn look_pending(&self) -> bool {
        self.is_sjk()
            && (self.draft.model != self.applied_model || self.draft.rgb != self.applied_rgb)
    }

    /// Drop a model chosen and not applied: the draft is the worn one again.
    pub(super) fn discard_look(&mut self) {
        if !self.look_pending() {
            return;
        }
        self.draft.model.clone_from(&self.applied_model);
        self.draft.rgb = self.applied_rgb;
        self.resolved_catalogue = false;
    }

    /// Match the `model` cvar against the catalogue once it is available.
    pub(super) fn resolve_current_choice(&mut self) {
        if self.resolved_catalogue {
            return;
        }
        let Some((choice, variants)) = self.catalog().map(|catalog| {
            // A model without a skin wears its default one, as the game reads it.
            let worn = if self.draft.model.contains(['/', '|']) {
                self.draft.model.clone()
            } else {
                format!("{}/default", self.draft.model)
            };
            let ordinary = catalog
                .characters
                .iter()
                .position(|entry| entry.cvar_value.eq_ignore_ascii_case(&worn));
            let species = catalog
                .species
                .iter()
                .enumerate()
                .find_map(|(index, species)| {
                    species_indices(species, &self.draft.model)
                        .map(|variants| (Choice::Species(index), variants))
                });
            if let Some(index) = ordinary {
                (Some(Choice::Character(index)), None)
            } else if let Some((choice, variants)) = species {
                (Some(choice), Some(variants))
            } else {
                // A model the catalogue lacks: the first default skin, so the grid
                // opens on Default rather than whatever team skin sorts first.
                let default = catalog
                    .characters
                    .iter()
                    .position(|entry| entry.skin.eq_ignore_ascii_case("default"));
                (
                    (!catalog.characters.is_empty())
                        .then_some(Choice::Character(default.unwrap_or(0))),
                    None,
                )
            }
        }) else {
            return;
        };
        if let Some(variants) = variants {
            self.variants[..3].copy_from_slice(&variants);
        }
        self.choice = choice;
        self.resolved_catalogue = true;
        self.team = self.team_of_choice();
        self.rebuild_tiles();
        self.grid_follow = true;
        self.creator.show_chosen();
    }

    /// Index of the current choice in the combined characters+species list.
    pub(super) fn choice_index(&self) -> usize {
        self.choice.map_or(0, |choice| match choice {
            Choice::Character(index) => index,
            Choice::Species(index) => self
                .catalog()
                .map_or(index, |catalog| catalog.characters.len() + index),
        })
    }

    /// Make the catalogue entry at `absolute` the draft model.
    pub(super) fn select_choice(&mut self, absolute: usize) {
        let Some(character_count) = self.catalog().map(|catalog| catalog.characters.len()) else {
            return;
        };
        if absolute < character_count {
            let value = self.catalog().unwrap().characters[absolute]
                .cvar_value
                .clone();
            self.choice = Some(Choice::Character(absolute));
            self.draft.model.clear();
            self.draft.model.push_str(&value);
        } else {
            let species_index = absolute - character_count;
            if self
                .catalog()
                .and_then(|catalog| catalog.species.get(species_index))
                .is_none()
            {
                return;
            }
            self.choice = Some(Choice::Species(species_index));
            self.variants = [0; 4];
            self.apply_species(species_index);
        }
        self.selected = self.selected.min(self.row_count().saturating_sub(1));
    }

    /// Step through the grid's tiles; a model the grid does not list (its
    /// skin belongs to another team) steps in from the grid's edge.
    pub(super) fn cycle_model(&mut self, direction: isize) {
        let total = self.tiles.len();
        if total == 0 {
            return;
        }
        let slot = match self.tile_position() {
            Some(slot) => wrap(slot, direction, total),
            None if direction > 0 => 0,
            None => total - 1,
        };
        self.select_choice(self.tiles[slot]);
        self.grid_follow = true;
    }

    /// Pointer pick of grid slot `slot`: make it the model; the SJK UI shows it, the
    /// others write it.
    pub(super) fn pick_tile(&mut self, console: &mut ViewerConsole, slot: usize) {
        let Some(&absolute) = self.tiles.get(slot) else {
            return;
        };
        self.select_choice(absolute);
        self.apply(console);
    }

    /// Step one species axis (0 heads, 1 torsos, 2 legs, 3 skin colours).
    pub(super) fn cycle_variant(&mut self, axis: usize, direction: isize) {
        let Some(Choice::Species(species_index)) = self.choice else {
            return;
        };
        let Some(species) = self
            .catalog()
            .and_then(|catalog| catalog.species.get(species_index))
        else {
            return;
        };
        let length = match axis {
            0 => species.heads.len(),
            1 => species.torsos.len(),
            2 => species.legs.len(),
            3 => species.colors.len(),
            _ => 0,
        };
        if length == 0 {
            return;
        }
        self.variants[axis] = wrap(self.variants[axis], direction, length);
        self.apply_species(species_index);
    }

    /// Choose variant `index` on species axis `axis` (0 heads, 1 torsos,
    /// 2 legs, 3 skin colours) of the current species.
    pub(super) fn set_variant(&mut self, axis: usize, index: usize) {
        let Some(Choice::Species(species_index)) = self.choice else {
            return;
        };
        if axis < self.variants.len() {
            self.variants[axis] = index;
            self.apply_species(species_index);
        }
    }

    fn apply_species(&mut self, species_index: usize) {
        let Some((model, rgb)) = self
            .catalog()
            .and_then(|catalog| catalog.species.get(species_index))
            .and_then(|species| {
                let (head, torso, legs) = selected_parts(species, self.variants)?;
                Some((
                    species.cvar_value(head, torso, legs),
                    species.colors.get(self.variants[3]).map(|color| color.rgb),
                ))
            })
        else {
            return;
        };
        self.draft.model.clear();
        self.draft.model.push_str(&model);
        if let Some(rgb) = rgb {
            self.draft.rgb = rgb;
        }
    }

    /// Step the selected row by `direction` and write the result at once.
    pub(super) fn adjust(&mut self, console: &mut ViewerConsole, direction: isize) {
        match self.page {
            ProfilePage::Character => {
                let Some(row) = self.character_rows().get(self.selected).copied() else {
                    return;
                };
                match row {
                    CharacterRow::Name | CharacterRow::Search | CharacterRow::Apply => {}
                    CharacterRow::Category => self.step_category(direction),
                    CharacterRow::Team => {
                        self.cycle_team(direction);
                        self.creator.show_chosen();
                    }
                    CharacterRow::Grid => self.grid_side(direction),
                }
            }
            ProfilePage::Saber => {
                let Some(row) = self.saber_rows().get(self.selected).copied() else {
                    return;
                };
                match row {
                    SaberRow::Search => return,
                    SaberRow::Skin => return self.blade_choice.step(console, direction),
                    _ => {}
                }
                let catalog = catalog_of(&self.loader);
                self.saber.adjust(row, direction, catalog);
                self.saber.apply(console);
            }
            // The Force page edits a draft; only its Apply action writes.
            ProfilePage::Force => match self.selected {
                // Light is the left card, Dark the right one.
                FORCE_SIDE_ROW => self.force.set_side(if direction < 0 {
                    ForceSide::Light
                } else {
                    ForceSide::Dark
                }),
                // Left and right walk the action buttons, which sit in a line.
                FORCE_RESET_ROW..=FORCE_APPLY_ROW => {
                    self.selected = self
                        .selected
                        .saturating_add_signed(direction)
                        .clamp(FORCE_RESET_ROW, FORCE_APPLY_ROW);
                }
                row => {
                    let power = row - FORCE_POWER_ROW;
                    if self.force.step(power, direction > 0) && direction > 0 {
                        self.note_level_bought(power);
                    }
                }
            },
        }
    }

    /// Enter/click on the selected row: edit the name, flip the Force side,
    /// run a Force action, or step a cycler forward.
    pub(super) fn activate(&mut self, console: &mut ViewerConsole) {
        if self.begin_numeric(self.selected) {
            return;
        }
        let search_row = match self.page {
            ProfilePage::Character => {
                self.character_rows().get(self.selected) == Some(&CharacterRow::Search)
            }
            ProfilePage::Saber => self.saber_rows().get(self.selected) == Some(&SaberRow::Search),
            ProfilePage::Force => false,
        };
        match (self.page, self.selected) {
            (ProfilePage::Character, 0) => {
                self.name_before_edit.clone_from(&self.draft.name);
                self.search_editing = false;
                self.name_editing = true;
            }
            _ if search_row => self.begin_search(),
            (ProfilePage::Character, row) if self.is_sjk() => {
                if self.character_rows().get(row) == Some(&CharacterRow::Apply) {
                    self.apply_look(console);
                    return;
                }
                match self.character_rows().get(row) {
                    // Enter on the strip goes down to the tiles.
                    Some(CharacterRow::Category) => self.selected = self.grid_row(),
                    Some(CharacterRow::Grid) => {
                        let position = self.creator_cursor();
                        self.creator_pick(console, position);
                    }
                    _ => self.adjust(console, 1),
                }
            }
            (ProfilePage::Force, FORCE_SIDE_ROW) => {
                let direction = match self.force.allocation().side {
                    ForceSide::Light => 1,
                    ForceSide::Dark => -1,
                };
                self.adjust(console, direction);
            }
            (ProfilePage::Force, FORCE_RESET_ROW) => self.force.reset(),
            (ProfilePage::Force, FORCE_DISCARD_ROW) => self.force.discard(),
            (ProfilePage::Force, FORCE_APPLY_ROW) => self.force.apply(console),
            _ => self.adjust(console, 1),
        }
    }

    pub(super) fn set_page(&mut self, page: ProfilePage) {
        self.numeric = None;
        self.page = page;
        self.selected = 0;
        self.name_editing = false;
        self.search_editing = false;
    }

    pub(crate) fn handle_key(
        &mut self,
        event: &KeyEvent,
        console: &mut ViewerConsole,
    ) -> PlayerMenuResult {
        if self.classic_style {
            return self.classic_key(event, console);
        }
        if event.state != ElementState::Pressed {
            return PlayerMenuResult::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return PlayerMenuResult::None;
        };
        if self.edit_numeric(key, event.text.as_deref(), event.repeat, console) {
            return PlayerMenuResult::None;
        }
        if self.name_editing {
            return self.edit_name(event, key, console);
        }
        if self.search_editing {
            match self.page {
                ProfilePage::Saber => self.edit_hilt_search(event, key),
                _ => self.edit_search(event, key),
            }
            return PlayerMenuResult::None;
        }
        if event.repeat {
            return PlayerMenuResult::None;
        }
        // SJK: a digit typed on a selected RGB row opens entry with it.
        if let Some(text) = event.text.as_deref()
            && text.starts_with(|c: char| c.is_ascii_digit())
            && self.begin_typed(self.selected, text)
        {
            return PlayerMenuResult::None;
        }
        let count = self.row_count().max(1);
        let pages = ProfilePage::ALL.len();
        match key {
            KeyCode::Escape => return self.escape(),
            KeyCode::Tab | KeyCode::BracketRight => {
                self.set_page(ProfilePage::ALL[(self.page.index() + 1) % pages]);
            }
            KeyCode::BracketLeft => {
                self.set_page(ProfilePage::ALL[(self.page.index() + pages - 1) % pages]);
            }
            // SJK UI: Up and Down move through the tiles of the grid by lines, and
            // on the Saber and Force pages through the rows in the order they show them.
            KeyCode::ArrowUp | KeyCode::KeyW if self.grid_step(-1) => {}
            KeyCode::ArrowDown | KeyCode::KeyS if self.grid_step(1) => {}
            KeyCode::ArrowUp | KeyCode::KeyW if self.is_sjk() && self.sjk_step(-1) => {}
            KeyCode::ArrowDown | KeyCode::KeyS if self.is_sjk() && self.sjk_step(1) => {}
            KeyCode::ArrowUp | KeyCode::KeyW => {
                self.selected = self.selected.checked_sub(1).unwrap_or(count - 1);
            }
            KeyCode::ArrowDown | KeyCode::KeyS => self.selected = (self.selected + 1) % count,
            KeyCode::ArrowLeft | KeyCode::KeyA => self.adjust(console, -1),
            KeyCode::ArrowRight | KeyCode::KeyD => self.adjust(console, 1),
            // SJK: Space keeps stepping an RGB row; Enter opens its entry.
            KeyCode::Space if self.is_channel_row(self.selected) => self.adjust(console, 1),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => self.activate(console),
            _ => {}
        }
        PlayerMenuResult::None
    }

    /// Escape outside a field: on the Saber page a hilt search kept after
    /// typing clears first (the SJK UI); else the screen closes.
    pub(super) fn escape(&mut self) -> PlayerMenuResult {
        if self.page == ProfilePage::Saber && !self.saber.search().is_empty() {
            self.saber.set_search("");
            return PlayerMenuResult::None;
        }
        PlayerMenuResult::Back(self.return_target)
    }

    pub(super) fn edit_name(
        &mut self,
        event: &KeyEvent,
        key: KeyCode,
        console: &mut ViewerConsole,
    ) -> PlayerMenuResult {
        match key {
            KeyCode::Escape => {
                self.draft.name.clone_from(&self.name_before_edit);
                self.name_editing = false;
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                self.name_editing = false;
                if self.draft.name.trim().is_empty() {
                    self.draft.name.clone_from(&self.name_before_edit);
                }
                self.apply(console);
            }
            KeyCode::Backspace => {
                self.draft.name.pop();
            }
            _ => {
                if let Some(text) = event.text.as_deref() {
                    append_legacy_name(&mut self.draft.name, text);
                }
            }
        }
        PlayerMenuResult::None
    }
}

pub(super) fn cvar_u8(console: &ViewerConsole, name: &str, fallback: u8) -> u8 {
    console
        .integer_cvar(name)
        .and_then(|value| u8::try_from(value).ok())
        .unwrap_or(fallback)
}

pub(super) fn wrap(index: usize, direction: isize, length: usize) -> usize {
    (index as isize + direction).rem_euclid(length as isize) as usize
}

fn selected_parts(species: &LegacySpecies, variants: [usize; 4]) -> Option<(&str, &str, &str)> {
    Some((
        species.heads.get(variants[0])?,
        species.torsos.get(variants[1])?,
        species.legs.get(variants[2])?,
    ))
}

fn species_indices(species: &LegacySpecies, model: &str) -> Option<[usize; 3]> {
    let parts = model.strip_prefix(&format!("{}/", species.model))?;
    let mut parts = parts.split('|');
    let head_name = parts.next()?;
    let torso_name = parts.next()?;
    let legs_name = parts.next()?;
    let head = species.heads.iter().position(|value| value == head_name)?;
    let torso = species
        .torsos
        .iter()
        .position(|value| value == torso_name)?;
    let legs = species.legs.iter().position(|value| value == legs_name)?;
    Some([head, torso, legs])
}

pub(super) fn append_legacy_name(name: &mut String, text: &str) {
    // OpenJK `q_shared.h` MAX_NAME_LENGTH is 32 including the NUL terminator.
    for character in text.chars().filter(|character| !character.is_control()) {
        if name.len() + character.len_utf8() > 31 {
            break;
        }
        name.push(character);
    }
}
