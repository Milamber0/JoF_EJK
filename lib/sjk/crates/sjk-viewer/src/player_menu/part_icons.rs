//! Character creation's pictures, as retail's `player2` menu drew them: each
//! head, torso and legs variant's `models/players/<species>/icon_<part>`
//! (`FEEDER_PLAYER_SKIN_HEAD` and the rest), and the species' tint base
//! (`gfx/menus/players/<species>/*tintbase`), which the colour swatches'
//! shaders multiply by the swatch colour. Only the species being edited is
//! loaded, into [`PART_CELLS`] atlas cells of its own; changing species
//! loads the next one there.

use super::icons::{IconLoader, IconRequest};
use crate::ui_renderer::{PART_ICON_CELLS, PART_ICON_FIRST};
use sjk_client::LegacySpecies;
use sjk_ui::TextureId;
use sjk_vfs::VirtualFileSystem;
use std::sync::Arc;

/// Atlas cells for one species: its parts, then its tint base.
pub(super) const PART_CELLS: usize = PART_ICON_CELLS as usize;
/// Image extensions retail tries for an icon (`bIsImageFile`).
const EXTENSIONS: [&str; 3] = ["jpg", "png", "tga"];

/// Which part list a picture belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Part {
    Head,
    Torso,
    Legs,
}

pub(super) struct PartIcons {
    loader: IconLoader,
    /// The species the cells hold (its catalogue index).
    species: Option<usize>,
    /// Parts per list, in cell order (heads, then torsos, then legs).
    counts: [usize; 3],
}

impl PartIcons {
    pub(super) fn new() -> Self {
        Self {
            loader: IconLoader::new(),
            species: None,
            counts: [0; 3],
        }
    }

    /// Load species `index`'s pictures unless the cells hold them already.
    pub(super) fn show(
        &mut self,
        vfs: &Arc<VirtualFileSystem>,
        index: usize,
        species: &LegacySpecies,
    ) {
        if self.species == Some(index) {
            return;
        }
        self.species = Some(index);
        self.counts = [
            species.heads.len(),
            species.torsos.len(),
            species.legs.len(),
        ];
        self.loader = IconLoader::new();
        self.loader
            .request_paths(Arc::clone(vfs), requests(vfs, species));
    }

    pub(super) fn poll(&mut self) {
        self.loader.poll();
    }

    /// Forget the uploaded pictures (the atlas is another world's): the
    /// next [`Self::show`] loads them again.
    pub(super) fn forget_uploads(&mut self) {
        self.species = None;
        self.loader = IconLoader::new();
    }

    pub(super) fn upload(
        &mut self,
        renderer: &crate::ui_renderer::ShapeRenderer,
        queue: &crate::frame_queue::FrameQueue,
    ) {
        self.loader.upload_batch(renderer, queue, 32);
    }

    /// The uploaded picture of variant `index` of `part`, for species `species`.
    pub(super) fn part(&self, species: usize, part: Part, index: usize) -> Option<TextureId> {
        let offset = match part {
            Part::Head => 0,
            Part::Torso => self.counts[0],
            Part::Legs => self.counts[0] + self.counts[1],
        };
        let count = self.counts[part as usize];
        // Parts past the range's last part cell have no picture.
        (index < count && offset + index < PART_CELLS - 1)
            .then(|| cell(offset + index))
            .flatten()
            .filter(|texture| self.ready(species, *texture))
    }

    /// The uploaded tint base of species `species`.
    pub(super) fn tint_base(&self, species: usize) -> Option<TextureId> {
        cell(PART_CELLS - 1).filter(|texture| self.ready(species, *texture))
    }

    fn ready(&self, species: usize, texture: TextureId) -> bool {
        self.species == Some(species) && self.loader.is_texture_ready(texture)
    }
}

/// Atlas cell `slot` of the range.
fn cell(slot: usize) -> Option<TextureId> {
    (slot < PART_CELLS).then(|| TextureId(PART_ICON_FIRST + slot as u32))
}

/// Every part picture of `species` (heads, torsos, legs; as many as fit
/// before the last cell) and its tint base in the last cell.
fn requests(vfs: &VirtualFileSystem, species: &LegacySpecies) -> Vec<IconRequest> {
    let mut requests: Vec<IconRequest> = species
        .heads
        .iter()
        .chain(&species.torsos)
        .chain(&species.legs)
        .take(PART_CELLS - 1)
        .enumerate()
        .filter_map(|(slot, part)| {
            let base = format!("models/players/{}/icon_{part}", species.model);
            Some((
                cell(slot)?,
                EXTENSIONS
                    .map(|extension| format!("{base}.{extension}"))
                    .to_vec(),
            ))
        })
        .collect();
    if let (Some(texture), Some(path)) = (cell(PART_CELLS - 1), tint_base_path(vfs, species)) {
        requests.push((texture, vec![path]));
    }
    requests
}

/// The image the species' tint swatch shaders multiply (`*tintbase` in
/// `gfx/menus/players/<species>/`, `hmtintbase.jpg` for `jedi_hm`).
fn tint_base_path(vfs: &VirtualFileSystem, species: &LegacySpecies) -> Option<String> {
    let folder = format!("gfx/menus/players/{}", species.model);
    EXTENSIONS.iter().find_map(|extension| {
        vfs.list_files(&folder, &format!(".{extension}"))
            .into_iter()
            .find(|file| !file.contains('/') && file.contains("tintbase"))
            .map(|file| format!("{folder}/{file}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn species() -> LegacySpecies {
        LegacySpecies {
            model: "jedi_hm".to_owned(),
            heads: vec!["head_a1".to_owned(), "head_b1".to_owned()],
            torsos: vec!["torso_a1".to_owned()],
            legs: vec!["lower_a1".to_owned(), "lower_b1".to_owned()],
            colors: Vec::new(),
        }
    }

    #[test]
    fn parts_take_cells_in_list_order_and_the_tint_base_the_last() {
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory(
            "assets1",
            [("gfx/menus/players/jedi_hm/hmtintbase.jpg", b"x".to_vec())],
        )
        .unwrap();
        let requests = requests(&vfs, &species());
        assert_eq!(requests.len(), 6);
        assert_eq!(requests[0].0, TextureId(PART_ICON_FIRST));
        assert_eq!(requests[0].1[0], "models/players/jedi_hm/icon_head_a1.jpg");
        assert_eq!(requests[2].1[1], "models/players/jedi_hm/icon_torso_a1.png");
        assert_eq!(requests[4].1[2], "models/players/jedi_hm/icon_lower_b1.tga");
        assert_eq!(
            requests[5].0,
            TextureId(PART_ICON_FIRST + PART_CELLS as u32 - 1)
        );
        assert_eq!(requests[5].1, ["gfx/menus/players/jedi_hm/hmtintbase.jpg"]);
        assert!(PART_ICON_FIRST + PART_ICON_CELLS <= crate::ui_renderer::ATLAS_CELLS);
    }

    #[test]
    fn nothing_shows_before_its_upload_or_for_another_species() {
        let icons = PartIcons {
            loader: IconLoader::new(),
            species: Some(3),
            counts: [2, 1, 2],
        };
        assert_eq!(icons.part(3, Part::Legs, 1), None);
        assert_eq!(icons.part(4, Part::Head, 0), None);
        assert_eq!(icons.tint_base(3), None);
        // Past the list's end there is no cell to draw.
        assert_eq!(icons.part(3, Part::Torso, 1), None);
    }
}
