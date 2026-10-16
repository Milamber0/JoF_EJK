//! Render-only area mask, composed with PVS (codemp/rd-vanilla/tr_world.cpp:1645-1654).
use sjk_bsp::{Bsp, Visibility};

/// Map-lifetime cluster membership and bounded protocol-26 snapshot mask.
#[derive(Default)]
pub(crate) struct Areas {
    clusters: Vec<Vec<i32>>,
    mask: [u8; 32],
    active: bool,
    revision: u64,
}

impl Areas {
    /// Retain all areas sharing each cluster; ambiguous membership fails open.
    pub(crate) fn new(bsp: &Bsp) -> Self {
        let count = bsp
            .leaves()
            .iter()
            .filter_map(|l| usize::try_from(l.cluster).ok())
            .max()
            .map_or(0, |n| n + 1)
            .min(bsp.leaves().len());
        let mut clusters = vec![Vec::new(); count];
        for leaf in bsp.leaves() {
            if let Ok(cluster) = usize::try_from(leaf.cluster) {
                // Corrupt/sparse indices must not allocate proportional to an arbitrary ID.
                let Some(areas) = clusters.get_mut(cluster) else {
                    continue;
                };
                if !areas.contains(&leaf.area) {
                    areas.push(leaf.area);
                }
            }
        }
        Self {
            clusters,
            mask: [0; 32],
            active: false,
            revision: 0,
        }
    }

    /// Copy accepted snapshot bits; absent/short masks leave unspecified areas visible.
    pub(crate) fn update(&mut self, mask: &[u8]) {
        let len = mask.len().min(self.mask.len());
        if self.mask[..len] == mask[..len] && self.mask[len..].iter().all(|&b| b == 0) {
            return;
        }
        self.revision = self.revision.wrapping_add(1);
        self.mask.fill(0);
        self.mask[..len].copy_from_slice(&mask[..len]);
        self.active = self.clusters.iter().flatten().any(|&area| !self.open(area));
    }

    /// Invalidate cached camera ranges whenever accepted area bits change.
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    /// Whether precomputed PVS-only spans must be bypassed.
    pub(crate) fn active(&self) -> bool {
        self.active
    }

    /// Current render mask, shared with secondary-view scheduling before any pass override.
    pub(crate) fn mask(&self) -> &[u8] {
        &self.mask
    }

    /// Whether the snapshot's area mask leaves `area` connected to the viewer's.
    pub(crate) fn open(&self, area: i32) -> bool {
        usize::try_from(area)
            .ok()
            .and_then(|n| self.mask.get(n >> 3).map(|byte| byte & (1 << (n & 7)) == 0))
            .unwrap_or(true)
    }

    /// PVS and area must agree for the same cluster; unknown membership draws conservatively.
    pub(crate) fn visible(
        &self,
        clusters: &[usize],
        source: Option<usize>,
        visibility: Option<&Visibility>,
    ) -> bool {
        if !self.active || source.is_none() {
            return super::visible(clusters, source, visibility);
        }
        clusters.is_empty()
            || clusters.iter().any(|&cluster| {
                super::visible(&[cluster], source, visibility)
                    && self.clusters.get(cluster).is_none_or(|areas| {
                        areas.is_empty() || areas.iter().any(|&area| self.open(area))
                    })
            })
    }
}
