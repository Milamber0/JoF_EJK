//! Who is shown to whom: an entity is linked to the clusters its box touches
//! (`SV_LinkEntity`, OpenJK `codemp/server/sv_world.cpp:288-340`) and a client is sent it
//! only if the cluster its eye is in can see one of them
//! (`SV_AddEntitiesVisibleFromPoint`, `sv_snapshot.cpp:330-420`). Held against
//! `tools/pvs-oracle`: the reference's own collision model over a synthetic map (checked
//! in) and over installed maps (opt-in).
//!
//! Areas are the parts of a map its doors cut apart (`cm_test.cpp:319-460`): every portal
//! starts closed, a door opens the portal between the two areas it stands in while it is
//! away from rest, and an entity in an area no open portal joins to the eye's is not
//! sent. Held against `tools/pvs-oracle/areas.cpp`, the reference's own collision model.
use sjk_bsp::Bsp;

/// `MAX_ENT_CLUSTERS`: how many clusters an entity names.
const NAMED: usize = 16;
/// `MAX_TOTAL_ENT_LEAFS`: how many leaves a link looks at.
const LEAVES: usize = 128;

/// What a client's eye can see: the PVS row of the cluster it is in, found once per
/// snapshot. A map without visibility data shows everything; an eye in no cluster (inside
/// a wall, outside the map) sees what the first cluster sees, as `CM_ClusterPVS` answers
/// for it.
#[derive(Clone, Copy, Debug)]
pub struct Eye<'a> {
    row: Option<&'a [u8]>,
    /// The area the eye is in (-1 for none), and the map's areas.
    area: i32,
    areas: Option<&'a Areas>,
}

impl<'a> Eye<'a> {
    /// The eye of a client: its origin raised by its view height.
    pub fn new(bsp: &'a Bsp, areas: &'a Areas, eye: [f32; 3]) -> Self {
        let leaf = &bsp.leaves()[bsp.leaf_at(eye)];
        let area = leaf.area;
        let Some(visibility) = bsp.render().visibility() else {
            return Self {
                row: None,
                area,
                areas: Some(areas),
            };
        };
        let row = usize::try_from(leaf.cluster)
            .ok()
            .filter(|cluster| *cluster < visibility.cluster_count)
            .unwrap_or(0);
        Self {
            row: visibility.cluster(row),
            area,
            areas: Some(areas),
        }
    }

    /// `SV_inPVS` (`sv_game.cpp:83-103`) from this eye to a point: the point's cluster is
    /// in the eye's row, and its area is joined to the eye's — a closed door blocks
    /// sight. A point in no cluster (in a wall, outside the map) is not seen; the
    /// reference reads outside its row there.
    pub fn sees_point(&self, bsp: &Bsp, point: [f32; 3]) -> bool {
        let leaf = &bsp.leaves()[bsp.leaf_at(point)];
        if let Some(row) = self.row {
            let cluster = leaf.cluster;
            if !(cluster >= 0
                && row
                    .get((cluster >> 3) as usize)
                    .is_some_and(|byte| byte & (1 << (cluster & 7)) != 0))
            {
                return false;
            }
        }
        self.areas
            .is_none_or(|areas| areas.connected(self.area, leaf.area))
    }

    /// The eye as the reference has it with `cm_noAreas 1`: every area joined to every
    /// other, only the clusters count.
    pub fn no_areas(bsp: &'a Bsp, eye: [f32; 3]) -> Self {
        let Some(visibility) = bsp.render().visibility() else {
            return Self::everywhere();
        };
        let cluster = bsp.leaves()[bsp.leaf_at(eye)].cluster;
        let row = usize::try_from(cluster)
            .ok()
            .filter(|cluster| *cluster < visibility.cluster_count)
            .unwrap_or(0);
        Self {
            row: visibility.cluster(row),
            area: -1,
            areas: None,
        }
    }

    /// The area the eye is in, -1 for none.
    pub fn area(&self) -> i32 {
        self.area
    }

    /// An eye that sees everything: no map is loaded.
    pub fn everywhere() -> Self {
        Self {
            row: None,
            area: -1,
            areas: None,
        }
    }
}

/// `CM_AdjustAreaPortalState`, `CM_FloodAreaConnections`, `CM_AreasConnected` and
/// `CM_WriteAreaBits` (`cm_test.cpp:319-460`): how many open portals join each pair of a
/// map's areas, and which areas that leaves joined. A map has as many areas as its
/// highest leaf area plus one (`CMod_LoadLeafs`).
#[derive(Clone, Debug, Default)]
pub struct Areas {
    count: usize,
    /// `areaPortals`: open portals between areas `a` and `b` at `a * count + b`.
    portals: Vec<i32>,
    /// `floodnum` of each area: two areas are joined when theirs are equal.
    flood: Vec<u32>,
}

impl Areas {
    /// The map's areas with every portal closed.
    pub fn new(bsp: &Bsp) -> Self {
        let count = bsp
            .leaves()
            .iter()
            .map(|leaf| leaf.area + 1)
            .max()
            .unwrap_or(0)
            .max(0) as usize;
        let mut areas = Self {
            count,
            portals: vec![0; count * count],
            flood: vec![0; count],
        };
        areas.flood();
        areas
    }

    /// How many areas the map has.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Every portal closed again, as a map loaded afresh has them.
    pub fn close_all(&mut self) {
        self.portals.fill(0);
        self.flood();
    }

    /// `CM_AdjustAreaPortalState`: one more (or one fewer) open portal between two areas.
    /// Nothing happens for an area of -1 (a door in no area). An area past the map's, or
    /// a portal closed more often than it was opened, is the reference's `ERR_DROP`; here
    /// it is left as it was.
    pub fn adjust(&mut self, first: i32, second: i32, open: bool) {
        let (Ok(first), Ok(second)) = (usize::try_from(first), usize::try_from(second)) else {
            return;
        };
        if first >= self.count || second >= self.count {
            return;
        }
        let step = if open { 1 } else { -1 };
        if self.portals[second * self.count + first] + step < 0 {
            return;
        }
        self.portals[first * self.count + second] += step;
        self.portals[second * self.count + first] += step;
        self.flood();
    }

    /// `CM_FloodAreaConnections`: each area not yet reached starts the next flood, which
    /// runs through every open portal.
    fn flood(&mut self) {
        let mut reached = vec![false; self.count];
        let mut stack = Vec::new();
        let mut number = 0;
        for start in 0..self.count {
            if reached[start] {
                continue;
            }
            number += 1;
            stack.push(start);
            reached[start] = true;
            while let Some(area) = stack.pop() {
                self.flood[area] = number;
                for other in 0..self.count {
                    if self.portals[area * self.count + other] > 0 && !reached[other] {
                        reached[other] = true;
                        stack.push(other);
                    }
                }
            }
        }
    }

    /// Each area's flood number, as the reference numbers them.
    pub fn floods(&self) -> &[u32] {
        &self.flood
    }

    /// `CM_AreasConnected`: two areas one flood reaches. An area of -1 is joined to none.
    pub fn connected(&self, first: i32, second: i32) -> bool {
        match (usize::try_from(first), usize::try_from(second)) {
            (Ok(first), Ok(second)) => self
                .flood
                .get(first)
                .is_some_and(|flood| self.flood.get(second) == Some(flood)),
            _ => false,
        }
    }

    /// `CM_WriteAreaBits`: the areas joined to `area` OR'd into `out` (every bit for an
    /// area of -1). Returns how many bytes the areas take.
    pub fn write_bits(&self, area: i32, out: &mut [u8]) -> usize {
        let bytes = self.count.div_ceil(8).min(out.len());
        match usize::try_from(area)
            .ok()
            .and_then(|area| self.flood.get(area))
        {
            None => out[..bytes].fill(255),
            Some(&flood) => {
                for (other, _) in self
                    .flood
                    .iter()
                    .enumerate()
                    .filter(|(other, number)| **number == flood && *other < bytes * 8)
                {
                    out[other >> 3] |= 1 << (other & 7);
                }
            }
        }
        bytes
    }
}

/// What `SV_LinkEntity` keeps of an entity for the PVS test.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClusterLink {
    clusters: [i32; NAMED],
    count: usize,
    /// The cluster of the last leaf, if the box touched more clusters than it could
    /// name; 0 otherwise (which the reference cannot tell from "cluster 0").
    last: i32,
    /// `areanum` and `areanum2`: the areas the box is in, -1 for none. A door may stand
    /// in two; a box in more keeps the first and the last.
    area: i32,
    area2: i32,
}

impl Default for ClusterLink {
    fn default() -> Self {
        Self {
            clusters: [0; NAMED],
            count: 0,
            last: 0,
            area: -1,
            area2: -1,
        }
    }
}

impl ClusterLink {
    /// Link a box: `absmin` and `absmax`, already grown by the unit `SV_LinkEntity` adds.
    pub fn new(bsp: &Bsp, absmin: [f32; 3], absmax: [f32; 3]) -> Self {
        let mut leaves = [0; LEAVES];
        let found = bsp.box_leaves(absmin, absmax, &mut leaves);
        let mut link = Self::default();
        // The areas, from every leaf, even those whose clusters are not named.
        for &leaf in &leaves[..found.count] {
            let area = bsp.leaves()[leaf].area;
            if area != -1 {
                if link.area != -1 && link.area != area {
                    link.area2 = area;
                } else {
                    link.area = area;
                }
            }
        }
        let mut looked_at = 0;
        for &leaf in &leaves[..found.count] {
            let cluster = bsp.leaves()[leaf].cluster;
            if cluster != -1 {
                link.clusters[link.count] = cluster;
                link.count += 1;
                if link.count == NAMED {
                    break;
                }
            }
            looked_at += 1;
        }
        if found.count > 0 && looked_at != found.count {
            link.last = found.last_leaf.map_or(0, |leaf| bsp.leaves()[leaf].cluster);
        }
        link
    }

    /// The clusters named.
    pub fn clusters(&self) -> &[i32] {
        &self.clusters[..self.count]
    }

    /// The two areas the box is in, -1 for none.
    pub fn areas(&self) -> (i32, i32) {
        (self.area, self.area2)
    }

    /// Whether `eye` is sent the entity: one of its areas joined to the eye's ("blocked
    /// by a door" otherwise), and one of its clusters in the eye's row.
    pub fn visible_from(&self, eye: &Eye<'_>) -> bool {
        if let Some(areas) = eye.areas
            && !areas.connected(eye.area, self.area)
            && !areas.connected(eye.area, self.area2)
        {
            return false;
        }
        if self.count == 0 {
            return false;
        }
        let Some(row) = eye.row else { return true };
        let seen = |cluster: i32| {
            row.get((cluster >> 3) as usize)
                .is_some_and(|byte| byte & (1 << (cluster & 7)) != 0)
        };
        if self.clusters().iter().any(|&cluster| seen(cluster)) {
            return true;
        }
        // "Check overflow clusters that couldn't be stored": every cluster number from the
        // last named one up to the last one is tried. The reference then hides the entity
        // only if the first visible one is the last cluster itself (`if ( l ==
        // svEnt->lastCluster )`) — so an entity too big to name its clusters is shown when
        // one in between is seen, and also when none is.
        let from = self.clusters[self.count - 1];
        self.last != 0
            && (from..=self.last)
                .find(|&cluster| seen(cluster))
                .is_none_or(|cluster| cluster != self.last)
    }
}
