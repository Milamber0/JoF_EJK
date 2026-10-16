//! Reuse nearby patch IDs while quadrature samples stay in the same spatial cell.
use glam::IVec3;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct Candidates {
    cell: Option<IVec3>,
    patch_count: usize,
    ids: Vec<usize>,
}

impl Candidates {
    /// Ascending IDs preserve the original search's lowest-eligible-patch choice.
    /// Adding any patch invalidates the list; patch moments are always tested live.
    pub(super) fn get(
        &mut self,
        cells: &HashMap<IVec3, Vec<usize>>,
        cell: IVec3,
        patch_count: usize,
    ) -> &[usize] {
        if self.cell != Some(cell) || self.patch_count != patch_count {
            self.ids.clear();
            for z in -1..=1 {
                for y in -1..=1 {
                    for x in -1..=1 {
                        if let Some(ids) = cells.get(&(cell + IVec3::new(x, y, z))) {
                            self.ids.extend_from_slice(ids);
                        }
                    }
                }
            }
            self.ids.sort_unstable();
            self.cell = Some(cell);
            self.patch_count = patch_count;
        }
        &self.ids
    }
}
