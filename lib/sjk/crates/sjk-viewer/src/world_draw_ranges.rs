//! Reuse final camera-visible ranges across geometry passes, with bounded retained storage.
use super::*;
use std::cell::{Ref, RefCell};

#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    camera: u64,
    area: u64,
    source: Option<usize>,
    pvs: bool,
}

#[derive(Default)]
struct Data {
    key: Option<Key>,
    // Area/PVS selection survives camera rotation and movement within one cluster.
    // Its indices refer to the immutable draw inventory selected by these same inputs.
    source_key: Option<(u64, Option<usize>, bool)>,
    source_draws: Vec<usize>,
    ranges: Vec<Range<u32>>,
}

/// One map-owned material cache, allocated before rendering and replaced by each new view.
#[derive(Default)]
pub(super) struct Cache {
    data: RefCell<Data>,
    enabled: bool,
}
impl Cache {
    fn reserve(&mut self, capacity: usize) {
        self.data.get_mut().ranges.reserve(capacity);
        self.data.get_mut().source_draws.reserve(capacity);
        self.enabled = capacity != 0;
    }
}

/// The materials a view from one PVS cluster can show at all. PVS and the area mask depend
/// on where the camera is, not on where it looks, so this set only changes when the
/// camera crosses a cluster, a door changes an area, or a material is added. Every
/// geometry pass of every camera walks this list instead of all of the map's materials.
#[derive(Default)]
pub(super) struct Active {
    key: Option<(Option<usize>, bool, u64, usize)>,
    /// Indexed by material; `list` holds the same set in material order.
    pub(super) flags: Vec<bool>,
    pub(super) list: Vec<usize>,
}

impl Active {
    /// Material classification changed; retain the allocated frame storage.
    pub(super) fn invalidate(&mut self) {
        self.key = None;
    }
}

impl Runtime {
    /// The active material set for `source`, refreshed only when its inputs change.
    pub(super) fn active_materials(
        &self,
        source: Option<usize>,
        visibility: Option<&Visibility>,
    ) -> Ref<'_, Active> {
        let key = (
            source,
            visibility.is_some(),
            self.areas.revision(),
            self.materials.len(),
        );
        if self.active.borrow().key != Some(key) {
            let mut active = self.active.borrow_mut();
            let active = &mut *active;
            active.flags.clear();
            active.list.clear();
            for (index, material) in self.materials.iter().enumerate() {
                // Materials outside the range cache (blended, unbounded, entity-only)
                // and movers keep their existing per-pass decisions.
                let shown = !material.camera_ranges.enabled
                    || !material.mover_draws.is_empty()
                    || !material.fog_draws.is_empty()
                    || {
                        let draws = if self.areas.active() {
                            &material.static_draws[..]
                        } else {
                            material.static_draws_for(source, visibility)
                        };
                        draws
                            .iter()
                            .any(|draw| self.areas.visible(&draw.clusters, source, visibility))
                    };
                active.flags.push(shown);
                if shown {
                    active.list.push(index);
                }
            }
            active.key = Some(key);
        }
        self.active.borrow()
    }

    /// Allocate once per map; each view reuses the original conservative draw inventory.
    pub(super) fn prepare_camera_ranges(&mut self) {
        for material in &mut self.materials {
            if material.view_bounded && !material.blended {
                material.camera_ranges.reserve(material.static_draws.len());
            }
        }
    }

    /// Yield this material's surviving ranges, sharing their order across passes in a view.
    pub(super) fn visible_static_ranges<'a>(
        &'a self,
        material: &'a Material,
        source: Option<usize>,
        visibility: Option<&'a Visibility>,
    ) -> impl Iterator<Item = Range<u32>> + 'a {
        let draws = if self.areas.active() {
            &material.static_draws[..]
        } else {
            material.static_draws_for(source, visibility)
        };
        if !material.camera_ranges.enabled {
            return Ranges::Direct {
                draws: draws.iter(),
                runtime: self,
                material,
                source,
                visibility,
            };
        }
        let key = Key {
            camera: self.view_culling.active_generation(),
            area: self.areas.revision(),
            source,
            pvs: visibility.is_some(),
        };
        if material.camera_ranges.data.borrow().key != Some(key) {
            let mut data = material.camera_ranges.data.borrow_mut();
            let source_key = (key.area, source, key.pvs);
            if data.source_key != Some(source_key) {
                data.source_draws.clear();
                for (index, draw) in draws.iter().enumerate() {
                    if self.areas.visible(&draw.clusters, source, visibility) {
                        data.source_draws.push(index);
                    }
                }
                data.source_key = Some(source_key);
            }
            let Data {
                source_draws,
                ranges,
                ..
            } = &mut *data;
            ranges.clear();
            for &index in source_draws.iter() {
                let draw = &draws[index];
                if !self.view_culling.cached(draw.bounds, &draw.view_cache) {
                    continue;
                }
                push_visible(ranges, draw.indices.clone());
            }
            data.key = Some(key);
        }
        let data = material.camera_ranges.data.borrow();

        Ranges::Cached { data, index: 0 }
    }
}

enum Ranges<'a> {
    Direct {
        draws: std::slice::Iter<'a, StaticDraw>,
        runtime: &'a Runtime,
        material: &'a Material,
        source: Option<usize>,
        visibility: Option<&'a Visibility>,
    },
    Cached {
        data: Ref<'a, Data>,
        index: usize,
    },
}
impl Iterator for Ranges<'_> {
    type Item = Range<u32>;
    fn next(&mut self) -> Option<Self::Item> {
        let range = match self {
            Self::Cached { data, index } => {
                let range = data.ranges.get(*index)?.clone();
                *index += 1;
                range
            }
            Self::Direct {
                draws,
                runtime,
                material,
                source,
                visibility,
            } => {
                let draw = draws.find(|draw| {
                    let visible = runtime.areas.visible(&draw.clusters, *source, *visibility)
                        && (!material.view_bounded
                            || runtime.view_culling.cached(draw.bounds, &draw.view_cache));

                    visible
                })?;
                draw.indices.clone()
            }
        };

        Some(range)
    }
}

/// Join only adjacent surviving ranges, retaining the original index order.
pub(super) fn push_visible(ranges: &mut Vec<Range<u32>>, range: Range<u32>) {
    if let Some(last) = ranges.last_mut().filter(|last| last.end == range.start) {
        last.end = range.end;
    } else {
        debug_assert!(ranges.len() < ranges.capacity());
        ranges.push(range);
    }
}
