//! Collision subdivision, not renderer tessellation. Port of OpenJK codemp
//! `cm_patch.cpp` (GPL-2.0-only); the 16-unit tolerance is gameplay-visible.
use super::dot;
use super::patch_geometry::{Point, sub};

pub(super) struct Grid {
    pub columns: Vec<Vec<Point>>,
    pub wrap_width: bool,
    pub wrap_height: bool,
}

impl Grid {
    pub fn generate(width: usize, height: usize, points: &[Point]) -> Result<Self, &'static str> {
        let mut grid = Self {
            columns: (0..width)
                .map(|x| (0..height).map(|y| points[y * width + x]).collect())
                .collect(),
            wrap_width: false,
            wrap_height: false,
        };
        grid.subdivide()?;
        let columns = (0..grid.height())
            .map(|y| grid.columns.iter().map(|c| c[y]).collect())
            .collect();
        grid.columns = columns;
        grid.wrap_height = grid.wrap_width;
        grid.subdivide()?;
        Ok(grid)
    }

    pub fn width(&self) -> usize {
        self.columns.len()
    }
    pub fn height(&self) -> usize {
        self.columns[0].len()
    }
    pub fn point(&self, x: usize, y: usize) -> Point {
        self.columns[x][y]
    }

    fn subdivide(&mut self) -> Result<(), &'static str> {
        self.wrap_width = self.columns[0]
            .iter()
            .zip(self.columns.last().unwrap())
            .all(|(a, b)| close(*a, *b));
        let mut i = 0;
        while i + 2 < self.width() {
            let needs = (0..self.height()).any(|y| {
                let a = self.point(i, y);
                let b = self.point(i + 1, y);
                let c = self.point(i + 2, y);
                let delta = sub(mid(mid(a, b), mid(b, c)), mid(a, c));
                dot(delta, delta) >= 16.0 * 16.0
            });
            if !needs {
                self.columns.remove(i + 1);
                i += 1;
                continue;
            }
            if self.width() + 2 > 129 {
                return Err("collision subdivision exceeds MAX_GRID_SIZE");
            }
            let mut left = Vec::with_capacity(self.height());
            let mut center = Vec::with_capacity(self.height());
            let mut right = Vec::with_capacity(self.height());
            for y in 0..self.height() {
                let a = mid(self.point(i, y), self.point(i + 1, y));
                let b = mid(self.point(i + 1, y), self.point(i + 2, y));
                left.push(a);
                center.push(mid(a, b));
                right.push(b);
            }
            self.columns.splice(i + 1..i + 2, [left, center, right]);
        }
        let mut i = 0;
        while i + 1 < self.width() {
            if self.columns[i]
                .iter()
                .zip(&self.columns[i + 1])
                .all(|(a, b)| close(*a, *b))
            {
                self.columns.remove(i + 1);
            } else {
                i += 1;
            }
        }
        Ok(())
    }
}

fn mid(a: Point, b: Point) -> Point {
    std::array::from_fn(|i| 0.5 * (a[i] + b[i]))
}
fn close(a: Point, b: Point) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() <= 0.1)
}
