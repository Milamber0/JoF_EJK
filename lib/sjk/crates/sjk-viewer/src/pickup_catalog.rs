//! Map-load model handles shared by pickups and carried flags.
use super::*;

/// Direct item-index to preloaded mesh lookup, built once per map.
pub(crate) struct Catalog {
    /// Carried CTF / CTY model handles, resolved at load time, not per actor.
    pub(crate) carrier_meshes: [[Option<usize>; 2]; 2],

    mesh_by_item: [Option<usize>; ITEM_COUNT],
    pub(super) holo_mesh: Option<usize>,
}

impl Catalog {
    /// Resolve item and carrier appearances once against successfully loaded rigid meshes.
    pub(crate) fn build(meshes: &[StaticModelMesh]) -> Self {
        let mesh_by_item = std::array::from_fn(|item_index| {
            let appearance = legacy_item_appearance(item_index as i16)?;
            meshes.iter().position(|mesh| mesh.appearance == appearance)
        });
        let holo_mesh = meshes
            .iter()
            .position(|mesh| mesh.appearance.model.eq_ignore_ascii_case(HOLO_MODEL));
        Self {
            carrier_meshes: std::array::from_fn(|mode| {
                std::array::from_fn(|team| {
                    meshes.iter().position(|mesh| {
                        mesh.appearance.model
                            == crate::actor_world_submission::flags::MODELS[mode][team]
                    })
                })
            }),

            mesh_by_item,
            holo_mesh,
        }
    }

    /// Return the retained item handle without a runtime model-name search.
    pub(super) fn mesh(&self, item_index: usize) -> Option<usize> {
        self.mesh_by_item.get(item_index).copied().flatten()
    }
}

/// Add every BaseJKA item model to the map-load appearance set once.
pub(crate) fn extend_appearances(appearances: &mut BTreeSet<Appearance>) {
    appearances.extend(
        crate::actor_world_submission::flags::MODELS[1].map(|model| Appearance {
            model: model.to_owned(),
            variant: String::new(),
        }),
    );
    appearances.extend((1..ITEM_COUNT).filter_map(|index| legacy_item_appearance(index as i16)));
    appearances.insert(Appearance {
        model: HOLO_MODEL.to_owned(),
        variant: String::new(),
    });
}
