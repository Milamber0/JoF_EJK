//! The blade edge a stock saber trail sweeps, stopped at walls like
//! `CG_AddSaberBlade`.
//!
//! Stock builds the blade `end` one unit past its length (`VectorMA(org_,
//! saberLen, axis_[0], end)` then `VectorAdd(end, axis_[0], end)` in
//! `codemp/cgame/cg_players.c`). With `cg_saberContact` on, a `MASK_SOLID`
//! trace from the muzzle to that `end` replaces it with the impact point
//! (`VectorCopy(trace.endpos, end)`), and the trail takes `end + 3·axis` as
//! its tip, both for the new slice and for the edge it remembers.
//! So a blade held into a wall leaves its trail on the near side of the wall
//! instead of sweeping a slice through it.

use crate::saber::Blade;
use glam::Vec3;

/// `CONTENTS_SOLID | CONTENTS_TERRAIN`, the client's `MASK_SOLID`.
const MASK_SOLID: u32 = 0x1 | 0x1000;

/// One trail edge: the blade muzzle and the trail tip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Edge {
    pub(crate) base: [f32; 3],
    pub(crate) tip: [f32; 3],
}

impl Edge {
    /// The edge of a blade whose (possibly clipped) stock `end` is `end`.
    pub(crate) fn from_end(blade: Blade, end: Vec3) -> Self {
        Self {
            base: blade.base,
            tip: (end + Vec3::from_array(blade.direction) * 3.0).to_array(),
        }
    }
}

/// Stock unclipped blade end: one unit past the visible length.
pub(crate) fn blade_end(blade: Blade) -> Vec3 {
    Vec3::from_array(blade.base) + Vec3::from_array(blade.direction) * (blade.length + 1.0)
}

/// Per-frame source of trail edges. Holds the world trace only while
/// `cg_saberContact` is on; the trace uses caller scratch, so a frame
/// allocates nothing.
pub(crate) struct Edges<'a> {
    contact: Option<(&'a sjk_bsp::Bsp, &'a mut sjk_bsp::TraceScratch)>,
}

impl<'a> Edges<'a> {
    /// `contact` is the world to clip against, or `None` with
    /// `cg_saberContact 0` (stock then jumps straight to `CheckTrail`).
    pub(crate) fn new(contact: Option<(&'a sjk_bsp::Bsp, &'a mut sjk_bsp::TraceScratch)>) -> Self {
        Self { contact }
    }

    /// The trail edge of `blade` this frame. Only world brushes clip it;
    /// stock's `CG_Trace` also stops at solid brush entities such as movers.
    pub(crate) fn edge(&mut self, blade: Blade) -> Edge {
        let end = blade_end(blade);
        let Some((bsp, scratch)) = self.contact.as_mut() else {
            return Edge::from_end(blade, end);
        };
        let trace = bsp.trace_box_with(
            scratch,
            blade.base,
            end.to_array(),
            sjk_bsp::Aabb::POINT,
            MASK_SOLID,
        );
        let end = if trace.fraction < 1.0 {
            Vec3::from_array(trace.end_position)
        } else {
            end
        };
        Edge::from_end(blade, end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blade() -> Blade {
        Blade {
            base: [10.0, 0.0, 0.0],
            direction: [0.0, 0.0, 1.0],
            length: 40.0,
            radius: 1.0,
        }
    }

    #[test]
    fn unclipped_tip_is_four_units_past_the_blade() {
        let edge = Edges::new(None).edge(blade());
        assert_eq!(edge.base, [10.0, 0.0, 0.0]);
        assert_eq!(edge.tip, [10.0, 0.0, 44.0]);
    }

    #[test]
    fn clipped_tip_is_three_units_past_the_impact() {
        let edge = Edge::from_end(blade(), Vec3::new(10.0, 0.0, 12.5));
        assert_eq!(edge.base, [10.0, 0.0, 0.0]);
        assert_eq!(edge.tip, [10.0, 0.0, 15.5]);
    }
}
