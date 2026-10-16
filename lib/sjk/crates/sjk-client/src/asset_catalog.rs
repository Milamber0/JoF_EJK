//! Lazy, mount-scoped compatibility catalogue for player-profile assets.
//!
//! Construction enumerates and reads the mounted VFS and is therefore kept on
//! an explicit worker. A loader instance belongs to one immutable VFS mount
//! set, so a completed result is reused until the caller replaces the loader.

use crate::character_catalog::{LegacyCharacter, LegacySpecies, legacy_character_catalog};
use crate::player_profile::SaberColor;
use crate::saber_definitions::{
    LegacySaberDefinition, legacy_saber_definitions, legacy_saber_load_order,
};
use crate::string_table;
use sjk_vfs::VirtualFileSystem;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

/// The six stable BaseJKA `saber_colors_t` values (`q_shared.h:349-358`).
pub const LEGACY_SABER_COLORS: [SaberColor; 6] = [
    SaberColor::Red,
    SaberColor::Orange,
    SaberColor::Yellow,
    SaberColor::Green,
    SaberColor::Blue,
    SaberColor::Purple,
];

/// Fully parsed compatibility catalogue used by player-profile screens.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyAssetCatalog {
    /// Icon-backed ordinary player model/skin choices, the retail game's first
    /// ([`Self::retail_characters`] of them), then the community's, each in
    /// alphabetical order.
    pub characters: Vec<LegacyCharacter>,
    /// How many of `characters`, from the first, are the retail game's own.
    pub retail_characters: usize,
    /// Multipart customisable species choices, the retail game's first.
    pub species: Vec<LegacySpecies>,
    /// How many of `species`, from the first, are the retail game's own.
    pub retail_species: usize,
    /// Saber definitions accepted by `UI_SaberValidForPlayerInMP`, in the game's
    /// load order with the retail game's first.
    pub saber_hilts: Vec<LegacySaberDefinition>,
    /// How many of `saber_hilts`, from the first, are the retail game's own.
    pub retail_saber_hilts: usize,
    /// Definitions excluded because their raw `notInMP` value is non-zero.
    pub excluded_saber_hilts: usize,
    /// The six numeric saber-colour choices sent in `color1` and `color2`.
    pub saber_colors: [SaberColor; 6],
}

/// Builds a catalogue synchronously.
///
/// UI callers should normally use [`LegacyAssetCatalogLoader`] so VFS reads do
/// not block the event thread. This entry point exists for workers and tools.
pub fn legacy_asset_catalog(
    vfs: &VirtualFileSystem,
) -> Result<LegacyAssetCatalog, Box<dyn std::error::Error + Send + Sync>> {
    let characters = legacy_character_catalog(vfs)?;
    let mut all_sabers =
        legacy_saber_definitions(vfs).map_err(|error| std::io::Error::other(error.to_string()))?;
    // `.sab` names are menu string-table references (`@MENUS_SINGLE_HILT1`).
    let strings = string_table::load_referenced(vfs, &["strings/english/menus.str"]);
    for definition in all_sabers.values_mut() {
        let name = string_table::resolve(&strings, &definition.display_name).to_owned();
        definition.display_name = name;
    }
    let excluded_saber_hilts = all_sabers
        .values()
        .filter(|definition| definition.not_in_mp)
        .count();
    let mut saber_hilts: Vec<LegacySaberDefinition> = all_sabers
        .into_values()
        .filter(|definition| !definition.not_in_mp)
        .collect();
    // In the order the game loads them, as EternalJK's menus list them; one
    // the load never met (none should) keeps its alphabetical place after.
    let order =
        legacy_saber_load_order(vfs).map_err(|error| std::io::Error::other(error.to_string()))?;
    let place: std::collections::HashMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    saber_hilts.sort_by_key(|hilt| {
        place
            .get(hilt.name.to_ascii_lowercase().as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });
    // The retail game's own items go first ([`crate::retail_assets`]): what the
    // retail archives alone catalogue, even where a community archive changes it.
    let mut characters = characters;
    let (retail_characters, retail_species, retail_saber_hilts) =
        match crate::retail_assets::retail_view(vfs) {
            Some(retail) => {
                let own = legacy_character_catalog(&retail)?;
                let own_hilts = legacy_saber_definitions(&retail)
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
                let own_characters: std::collections::HashSet<&str> = own
                    .characters
                    .iter()
                    .map(|each| each.cvar_value.as_str())
                    .collect();
                let own_species: std::collections::HashSet<&str> =
                    own.species.iter().map(|each| each.model.as_str()).collect();
                let characters_first =
                    crate::retail_assets::retail_first(&mut characters.characters, |character| {
                        own_characters.contains(character.cvar_value.as_str())
                    });
                let species_first =
                    crate::retail_assets::retail_first(&mut characters.species, |species| {
                        own_species.contains(species.model.as_str())
                    });
                let hilts_first = crate::retail_assets::retail_first(&mut saber_hilts, |hilt| {
                    own_hilts.contains_key(&hilt.name.to_ascii_lowercase())
                });
                (characters_first, species_first, hilts_first)
            }
            None => (0, 0, 0),
        };
    Ok(LegacyAssetCatalog {
        characters: characters.characters,
        retail_characters,
        species: characters.species,
        retail_species,
        saber_hilts,
        retail_saber_hilts,
        excluded_saber_hilts,
        saber_colors: LEGACY_SABER_COLORS,
    })
}

/// State of a mount-scoped asynchronous catalogue request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyCatalogStatus {
    /// No request has been made; no worker exists and no VFS read has occurred.
    Idle,
    /// The worker is enumerating and parsing the mounted assets.
    Loading,
    /// The parsed catalogue is cached and available through `catalog`.
    Ready,
    /// The worker failed; the stable diagnostic is available through `error`.
    Failed,
}

/// One lazy catalogue cache bound to a single VFS mount set.
pub struct LegacyAssetCatalogLoader {
    vfs: Arc<VirtualFileSystem>,
    receiver: Option<Receiver<Result<LegacyAssetCatalog, String>>>,
    catalog: Option<Arc<LegacyAssetCatalog>>,
    error: Option<String>,
    status: LegacyCatalogStatus,
}

impl LegacyAssetCatalogLoader {
    /// Creates an idle loader without enumerating or reading the VFS.
    pub fn new(vfs: Arc<VirtualFileSystem>) -> Self {
        Self {
            vfs,
            receiver: None,
            catalog: None,
            error: None,
            status: LegacyCatalogStatus::Idle,
        }
    }

    /// Starts exactly one background build; ready and loading requests are no-ops.
    pub fn request(&mut self) {
        if self.status != LegacyCatalogStatus::Idle {
            return;
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let vfs = Arc::clone(&self.vfs);
        std::thread::spawn(move || {
            let result = legacy_asset_catalog(&vfs).map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
        self.receiver = Some(receiver);
        self.status = LegacyCatalogStatus::Loading;
    }

    /// Polls the worker without blocking and caches a completed result.
    pub fn poll(&mut self) -> LegacyCatalogStatus {
        if self.status != LegacyCatalogStatus::Loading {
            return self.status;
        }
        let Some(receiver) = self.receiver.as_ref() else {
            return self.status;
        };
        match receiver.try_recv() {
            Ok(Ok(catalog)) => {
                self.catalog = Some(Arc::new(catalog));
                self.receiver = None;
                self.status = LegacyCatalogStatus::Ready;
            }
            Ok(Err(error)) => {
                self.error = Some(error);
                self.receiver = None;
                self.status = LegacyCatalogStatus::Failed;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.error = Some("catalogue worker disconnected".to_owned());
                self.receiver = None;
                self.status = LegacyCatalogStatus::Failed;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        self.status
    }

    /// Returns the current state without polling or allocating.
    pub fn status(&self) -> LegacyCatalogStatus {
        self.status
    }

    /// Returns the cached catalogue after the loader reaches `Ready`.
    pub fn catalog(&self) -> Option<&Arc<LegacyAssetCatalog>> {
        self.catalog.as_ref()
    }

    /// Returns the stable worker diagnostic after the loader reaches `Failed`.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Against the installed game (`JKA_GAME_DATA`): the retail items come first,
    /// then the community's, and a hilt of each kind sits on its side of the line.
    #[test]
    #[ignore = "reads the installed game data named by JKA_GAME_DATA"]
    fn retail_items_lead_the_catalogue() {
        let Ok(data) = std::env::var("JKA_GAME_DATA") else {
            return;
        };
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_pk3_directory(std::path::Path::new(&data).join("base"))
            .unwrap();
        let catalog = legacy_asset_catalog(&vfs).unwrap();
        println!(
            "characters {} (retail {}), species {} (retail {}), hilts {} (retail {})",
            catalog.characters.len(),
            catalog.retail_characters,
            catalog.species.len(),
            catalog.retail_species,
            catalog.saber_hilts.len(),
            catalog.retail_saber_hilts
        );
        let names = |count: usize| {
            catalog.characters[..count.min(6)]
                .iter()
                .map(|each| each.cvar_value.clone())
                .collect::<Vec<_>>()
        };
        println!("first {:?}", names(catalog.retail_characters));
        println!(
            "first community {:?}",
            catalog.characters[catalog.retail_characters..]
                .iter()
                .take(4)
                .map(|each| each.cvar_value.clone())
                .collect::<Vec<_>>()
        );
        println!(
            "hilts {:?} / {:?}",
            catalog
                .saber_hilts
                .iter()
                .take(4)
                .map(|hilt| hilt.name.clone())
                .collect::<Vec<_>>(),
            catalog.saber_hilts[catalog.retail_saber_hilts..]
                .iter()
                .take(4)
                .map(|hilt| hilt.name.clone())
                .collect::<Vec<_>>()
        );
        assert!(catalog.retail_characters > 0);
        assert!(catalog.retail_characters < catalog.characters.len());
    }
}
