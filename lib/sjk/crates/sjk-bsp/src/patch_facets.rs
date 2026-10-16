//! Facet construction from codemp `cm_patch.cpp`, including shared borders,
//! degenerate triangles, axial/edge bevels and the one-sided back plane.
use super::patch_geometry::*;
use super::patch_grid::Grid;
use super::{Plane, dot};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Border {
    pub plane: usize,
    pub inward: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Facet {
    pub surface: usize,
    pub borders: Box<[Border]>,
}

#[derive(Default)]
pub(super) struct Builder {
    pub planes: Vec<Plane>,
    pub facets: Vec<Facet>,
}

impl Builder {
    fn push_plane(&mut self, plane: Plane) -> Result<usize, &'static str> {
        if self.planes.len() == 4096 {
            return Err("MAX_PATCH_PLANES exceeded");
        }
        self.planes.push(plane);
        Ok(self.planes.len() - 1)
    }

    fn find_points(&mut self, a: Point, b: Point, c: Point) -> Result<Option<usize>, &'static str> {
        let Some(plane) = from_points(a, b, c) else {
            return Ok(None);
        };
        for (i, p) in self.planes.iter().enumerate() {
            if dot(plane.normal, p.normal) < 0.0 {
                continue;
            }
            if [a, b, c].iter().all(|v| p.signed_distance(*v).abs() <= 0.1) {
                return Ok(Some(i));
            }
        }
        self.push_plane(plane).map(Some)
    }

    fn find_plane(&mut self, plane: Plane) -> Result<Border, &'static str> {
        for (index, p) in self.planes.iter().enumerate() {
            if let Some(inward) = equal(*p, plane) {
                return Ok(Border {
                    plane: index,
                    inward,
                });
            }
        }
        Ok(Border {
            plane: self.push_plane(plane)?,
            inward: false,
        })
    }

    pub fn generate(grid: &Grid) -> Result<Self, &'static str> {
        let mut out = Self::default();
        let width = grid.width().saturating_sub(1);
        let height = grid.height().saturating_sub(1);
        let mut cells = vec![[None; 2]; width * height];
        for x in 0..width {
            for y in 0..height {
                cells[x * height + y] = [
                    out.find_points(
                        grid.point(x, y),
                        grid.point(x + 1, y),
                        grid.point(x + 1, y + 1),
                    )?,
                    out.find_points(
                        grid.point(x + 1, y + 1),
                        grid.point(x, y + 1),
                        grid.point(x, y),
                    )?,
                ];
            }
        }
        for x in 0..width {
            for y in 0..height {
                let cell = cells[x * height + y];
                let at = |x: usize, y: usize, tri: usize| cells[x * height + y][tri];
                // TOP, RIGHT, BOTTOM, LEFT. Keep the reference's plane insertion order.
                let mut borders = [None; 4];
                borders[0] = if y > 0 {
                    at(x, y - 1, 1)
                } else if grid.wrap_height {
                    at(x, height - 1, 1)
                } else {
                    None
                };
                borders[2] = if y + 1 < height {
                    at(x, y + 1, 0)
                } else if grid.wrap_height {
                    at(x, 0, 0)
                } else {
                    None
                };
                borders[3] = if x > 0 {
                    at(x - 1, y, 0)
                } else if grid.wrap_width {
                    at(width - 1, y, 0)
                } else {
                    None
                };
                borders[1] = if x + 1 < width {
                    at(x + 1, y, 1)
                } else if grid.wrap_width {
                    at(0, y, 1)
                } else {
                    None
                };
                for edge in [0, 2, 3, 1] {
                    let tri = usize::from(edge == 2 || edge == 3);
                    if borders[edge].is_none() || borders[edge] == cell[tri] {
                        borders[edge] = out.edge(grid, x, y, edge, cell)?;
                    }
                }
                let points = [
                    grid.point(x, y),
                    grid.point(x + 1, y),
                    grid.point(x + 1, y + 1),
                    grid.point(x, y + 1),
                ];
                if cell[0] == cell[1] {
                    out.facet(cell[0], &borders, &points)?;
                } else {
                    let diagonal = if let Some(p) = cell[1].or(borders[2]) {
                        Some(p)
                    } else {
                        out.edge(grid, x, y, 4, cell)?
                    };
                    out.facet(cell[0], &[borders[0], borders[1], diagonal], &points[..3])?;
                    let diagonal = if let Some(p) = cell[0].or(borders[0]) {
                        Some(p)
                    } else {
                        out.edge(grid, x, y, 5, cell)?
                    };
                    out.facet(
                        cell[1],
                        &[borders[2], borders[3], diagonal],
                        &[points[2], points[3], points[0]],
                    )?;
                }
            }
        }
        Ok(out)
    }

    fn edge(
        &mut self,
        g: &Grid,
        x: usize,
        y: usize,
        edge: usize,
        cell: [Option<usize>; 2],
    ) -> Result<Option<usize>, &'static str> {
        let (a, b, tri, reverse) = match edge {
            0 => (g.point(x, y), g.point(x + 1, y), 0, false),
            1 => (g.point(x + 1, y), g.point(x + 1, y + 1), 0, false),
            2 => (g.point(x, y + 1), g.point(x + 1, y + 1), 1, true),
            3 => (g.point(x, y), g.point(x, y + 1), 1, true),
            4 => (g.point(x + 1, y + 1), g.point(x, y), 0, false),
            _ => (g.point(x, y), g.point(x + 1, y + 1), 1, false),
        };
        let Some(p) = cell[tri].or(cell[tri ^ 1]) else {
            return Ok(None);
        };
        let up = add(a, scale(self.planes[p].normal, 4.0));
        if reverse {
            self.find_points(b, a, up)
        } else {
            self.find_points(a, b, up)
        }
    }

    fn facet(
        &mut self,
        surface: Option<usize>,
        edges: &[Option<usize>],
        points: &[Point],
    ) -> Result<(), &'static str> {
        let Some(surface) = surface else {
            return Ok(());
        };
        if self.facets.len() == 1024 {
            return Err("MAX_FACETS exceeded");
        }
        let mut borders = Vec::with_capacity(26);
        for edge in edges {
            let Some(plane) = *edge else {
                return Ok(());
            };
            let p = self.planes[plane];
            let front = points.iter().any(|v| p.signed_distance(*v) > 0.1);
            let back = points.iter().any(|v| p.signed_distance(*v) < -0.1);
            if !front && !back {
                return Ok(());
            }
            borders.push(Border {
                plane,
                inward: front && !back,
            });
        }
        let mut winding = base_winding(self.planes[surface]);
        for border in &borders {
            let p = self.planes[border.plane];
            chop(&mut winding, if border.inward { p } else { flipped(p) });
            if winding.is_empty() {
                return Ok(());
            }
        }
        let b = bounds(&winding);
        if (0..3).any(|i| {
            b[1][i] - b[0][i] > MAP_BOUNDS || b[0][i] >= MAP_BOUNDS || b[1][i] <= -MAP_BOUNDS
        }) {
            return Ok(());
        }
        self.bevels(surface, &mut borders, &winding)?;
        borders.push(Border {
            plane: surface,
            inward: true,
        });
        self.facets.push(Facet {
            surface,
            borders: borders.into_boxed_slice(),
        });
        Ok(())
    }

    fn has_plane(&self, surface: usize, borders: &[Border], plane: Plane) -> bool {
        equal(self.planes[surface], plane).is_some()
            || borders
                .iter()
                .any(|b| equal(self.planes[b.plane], plane).is_some())
    }

    fn bevels(
        &mut self,
        surface: usize,
        borders: &mut Vec<Border>,
        winding: &[Point],
    ) -> Result<(), &'static str> {
        let b = bounds(winding);
        for axis in 0..3 {
            for dir in [-1.0, 1.0] {
                let mut normal = [0.0; 3];
                normal[axis] = dir;
                let plane = Plane {
                    normal,
                    distance: if dir == 1.0 { b[1][axis] } else { -b[0][axis] },
                };
                if !self.has_plane(surface, borders, plane) {
                    borders.push(self.find_plane(plane)?);
                }
            }
        }
        for j in 0..winding.len() {
            let mut edge = sub(winding[j], winding[(j + 1) % winding.len()]);
            if normalize(&mut edge) < 0.5 {
                continue;
            }
            for k in 0..3 {
                if (edge[k].abs() - 1.0).abs() < NORMAL_EPSILON {
                    let dir = edge[k].signum();
                    edge = [0.0; 3];
                    edge[k] = dir;
                    break;
                }
            }
            if edge.iter().any(|v| v.abs() == 1.0) {
                continue;
            }
            for axis in 0..3 {
                for dir in [-1.0, 1.0] {
                    let mut axial = [0.0; 3];
                    axial[axis] = dir;
                    let mut normal = cross(edge, axial);
                    if normalize(&mut normal) < 0.5 {
                        continue;
                    }
                    let plane = Plane {
                        normal,
                        distance: dot(winding[j], normal),
                    };
                    if winding.iter().any(|p| plane.signed_distance(*p) > 0.1)
                        || self.has_plane(surface, borders, plane)
                    {
                        continue;
                    }
                    let border = self.find_plane(plane)?;
                    let p = self.planes[border.plane];
                    let mut test = winding.to_vec();
                    chop(&mut test, if border.inward { p } else { flipped(p) });
                    if !test.is_empty() {
                        borders.push(border);
                    }
                }
            }
        }
        Ok(())
    }
}
