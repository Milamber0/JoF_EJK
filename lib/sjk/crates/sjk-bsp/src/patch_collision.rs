//! Owned quadratic-patch collision. Algorithms ported from OpenJK codemp
//! `cm_patch.cpp` (GPL-2.0-only); no renderer LOD or process-global state.
use super::collision::TraceWork;
use super::patch_facets::{Builder, Facet};
use super::patch_geometry::{bounds, flipped};
use super::patch_grid::Grid;
use super::{BspError, CollisionTrace, Plane, RenderData, Shader, dot};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Patch {
    bounds: [[f32; 3]; 2],
    planes: Box<[Plane]>,
    facets: Box<[Facet]>,
    contents: u32,
    flags: u32,
    /// The patch's shader, an index into the BSP's shaders.
    shader: usize,
}

pub(super) fn load(
    render: &RenderData,
    shaders: &[Shader],
) -> Result<Box<[Option<Patch>]>, BspError> {
    render
        .surfaces()
        .iter()
        .enumerate()
        .map(|(index, surface)| {
            let Some([width, height]) = surface.patch_dimensions else {
                return Ok(None);
            };
            let points: Vec<_> = render.vertices()[surface.vertices.clone()]
                .iter()
                .map(|v| v.position)
                .collect();
            Patch::generate(width, height, &points, &shaders[surface.shader])
                .map(|patch| {
                    Some(Patch {
                        shader: surface.shader,
                        ..patch
                    })
                })
                .map_err(|reason| BspError::PatchCollision {
                    surface: index,
                    reason,
                })
        })
        .collect()
}

impl Patch {
    /// The patch's content flags (its shader's).
    pub(crate) fn contents(&self) -> u32 {
        self.contents
    }

    fn generate(
        width: usize,
        height: usize,
        points: &[[f32; 3]],
        shader: &Shader,
    ) -> Result<Self, &'static str> {
        let grid = Grid::generate(width, height, points)?;
        let points: Vec<_> = grid.columns.iter().flatten().copied().collect();
        let mut bounds = bounds(&points);
        for i in 0..3 {
            bounds[0][i] -= 1.0;
            bounds[1][i] += 1.0;
        }
        let builder = Builder::generate(&grid)?;
        Ok(Self {
            bounds,
            planes: builder.planes.into_boxed_slice(),
            facets: builder.facets.into_boxed_slice(),
            contents: shader.content_flags,
            flags: shader.surface_flags,
            shader: 0,
        })
    }

    pub fn trace(&self, work: &TraceWork, mask: u32, trace: &mut CollisionTrace) {
        if self.contents & mask == 0 {
            return;
        }
        if (0..3).any(|i| {
            work.start[i].min(work.end[i]) - work.extents[i] > self.bounds[1][i]
                || work.start[i].max(work.end[i]) + work.extents[i] < self.bounds[0][i]
        }) {
            return;
        }
        if work.start == work.end {
            if !work.is_point
                && self.facets.iter().any(|facet| {
                    self.expanded(work, self.planes[facet.surface])
                        .signed_distance(work.start)
                        <= 0.0
                        && facet.borders.iter().all(|b| {
                            let p = self.planes[b.plane];
                            self.expanded(work, if b.inward { flipped(p) } else { p })
                                .signed_distance(work.start)
                                <= 0.0
                        })
                })
            {
                trace.start_solid = true;
                trace.all_solid = true;
                trace.fraction = 0.0;
                trace.content_flags = self.contents;
            }
            return;
        }
        let previous = trace.fraction;
        if work.is_point {
            self.trace_point(work, trace);
        } else {
            self.trace_box(work, trace);
        }
        if trace.fraction < previous {
            trace.content_flags = self.contents;
            trace.surface_flags = self.flags;
            trace.shader = Some(self.shader);
        }
    }

    fn expanded(&self, work: &TraceWork, mut plane: Plane) -> Plane {
        plane.distance -= dot(work.corner_for_plane(plane), plane.normal);
        plane
    }

    fn trace_box(&self, work: &TraceWork, trace: &mut CollisionTrace) {
        for facet in &self.facets {
            let mut enter = -1.0;
            let mut leave = 1.0;
            let surface = self.expanded(work, self.planes[facet.surface]);
            let mut best = surface;
            if check_plane(surface, work, &mut enter, &mut leave).is_none() {
                continue;
            }
            let mut hit_border = None;
            let mut misses = false;
            for (j, border) in facet.borders.iter().enumerate() {
                let plane = self.planes[border.plane];
                let plane = self.expanded(work, if border.inward { flipped(plane) } else { plane });
                match check_plane(plane, work, &mut enter, &mut leave) {
                    None => {
                        misses = true;
                        break;
                    }
                    Some(true) => {
                        best = plane;
                        hit_border = Some(j);
                    }
                    Some(false) => {}
                }
            }
            if misses || hit_border == Some(facet.borders.len() - 1) {
                continue;
            }
            if enter < leave && enter >= 0.0 && enter < trace.fraction {
                trace.fraction = enter;
                trace.plane = Some(best);
            }
        }
    }

    fn trace_point(&self, work: &TraceWork, trace: &mut CollisionTrace) {
        let relation = |plane: Plane| {
            let d1 = plane.signed_distance(work.start);
            let d2 = plane.signed_distance(work.end);
            let mut intersection = if d1 == d2 { 99999.0 } else { d1 / (d1 - d2) };
            if intersection <= 0.0 {
                intersection = 99999.0;
            }
            (d1 > 0.0, intersection, d1, d2)
        };
        for facet in &self.facets {
            let plane = self.planes[facet.surface];
            let (front, intersection, d1, d2) = relation(plane);
            if !front || intersection < 0.0 || intersection > trace.fraction {
                continue;
            }
            if facet.borders.iter().any(|border| {
                let (front, crossing, _, _) = relation(self.planes[border.plane]);
                if front ^ border.inward {
                    crossing > intersection
                } else {
                    crossing < intersection
                }
            }) {
                continue;
            }
            trace.fraction = ((d1 - 0.125) / (d1 - d2)).max(0.0);
            trace.plane = Some(plane);
        }
    }
}

fn check_plane(plane: Plane, work: &TraceWork, enter: &mut f32, leave: &mut f32) -> Option<bool> {
    let d1 = plane.signed_distance(work.start);
    let d2 = plane.signed_distance(work.end);
    if d1 > 0.0 && (d2 >= 0.125 || d2 >= d1) {
        return None;
    }
    if d1 <= 0.0 && d2 <= 0.0 {
        return Some(false);
    }
    if d1 > d2 {
        let f = ((d1 - 0.125) / (d1 - d2)).max(0.0);
        if f > *enter {
            *enter = f;
            return Some(true);
        }
    } else {
        *leave = leave.min(((d1 + 0.125) / (d1 - d2)).min(1.0));
    }
    Some(false)
}
