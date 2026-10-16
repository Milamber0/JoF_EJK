//! The far cover: a coarse survey of the whole map, for the fog.
//!
//! The fog marches rays up to 6000 units, past the near cover's window
//! (`weather_cover_map.rs`). Beyond the window, and in near columns that hold none of
//! the map's air, it reads this grid instead: [`SIZE`]² columns centred on the map, each
//! at least [`MIN_CELL`] units wide, surveyed once per map on a worker thread. Columns
//! beyond the map's walls all take one span, the map's middle floor up to its highest
//! sky, so fog and haze carry on to the horizon as one level bank around the map. Nothing
//! here depends on the camera: distant fog stays where it lies however the player moves.

use super::cover::{Column, Surveyor};
use std::sync::mpsc::{Receiver, TryRecvError};

/// Columns along each edge of the grid.
pub(crate) const SIZE: usize = 256;
/// The narrowest column, in world units.
const MIN_CELL: f32 = 64.0;

/// Where the grid lies in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Grid {
    /// The first column's corner.
    pub(crate) origin: [f32; 2],
    /// Column width: a multiple of 16, at least [`MIN_CELL`].
    pub(crate) cell: f32,
}

impl Grid {
    /// The grid centred on the map's box, with the narrowest columns that cover it.
    pub(crate) fn around(bounds: [[f32; 3]; 2]) -> Self {
        let [low, high] = bounds;
        let extent = (high[0] - low[0]).max(high[1] - low[1]).max(0.0);
        let cell = ((extent / SIZE as f32 / 16.0).ceil() * 16.0).max(MIN_CELL);
        let half = 0.5 * cell * SIZE as f32;
        Self {
            origin: [0, 1].map(|axis| 0.5 * (low[axis] + high[axis]) - half),
            cell,
        }
    }

    /// The centre of the column at `index` (row by row).
    fn centre(self, index: usize) -> [f32; 2] {
        let cell = [index % SIZE, index / SIZE];
        [0, 1].map(|axis| self.origin[axis] + (cell[axis] as f32 + 0.5) * self.cell)
    }
}

/// Survey every column of `grid` inside the map's box, then fill those beyond its walls.
/// `None` once `stop`'s sender is dropped: the map's cover was replaced.
pub(crate) fn survey(
    surveyor: &mut Surveyor,
    grid: Grid,
    bounds: [[f32; 3]; 2],
    stop: &Receiver<()>,
) -> Option<Box<[[f32; 4]]>> {
    let [low, high] = bounds;
    let mut columns = vec![Column::OUTSIDE_MAP; SIZE * SIZE];
    for (row, line) in columns.chunks_mut(SIZE).enumerate() {
        if let Err(TryRecvError::Disconnected) = stop.try_recv() {
            return None;
        }
        for (index, column) in (row * SIZE..).zip(line) {
            let [x, y] = grid.centre(index);
            if (low[0]..=high[0]).contains(&x) && (low[1]..=high[1]).contains(&y) {
                *column = surveyor.column(x, y);
            }
        }
    }
    fill_beyond(&mut columns);
    Some(columns.iter().map(|column| column.texel()).collect())
}

/// Give every column outside the map one span: the middle (median) floor of the open
/// columns up to the highest sky. A level bank of fog then lies around the map, not a
/// copy of whichever roof or pit meets the wall. Covered columns are left as they are,
/// and a map without open columns keeps its void empty.
fn fill_beyond(columns: &mut [Column]) {
    let mut floors: Vec<f32> = columns
        .iter()
        .filter(|column| column.is_open())
        .map(|column| column.bottom)
        .collect();
    if floors.is_empty() {
        return;
    }
    let middle = floors.len() / 2;
    let floor = *floors.select_nth_unstable_by(middle, f32::total_cmp).1;
    let sky = columns
        .iter()
        .filter(|column| column.is_open())
        .map(|column| column.top)
        .fold(f32::MIN, f32::max);
    for column in columns
        .iter_mut()
        .filter(|column| column.flags & Column::VOID != 0)
    {
        *column = Column {
            bottom: floor,
            top: sky,
            flags: 0,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_centres_on_the_map_with_columns_wide_enough_to_cover_it() {
        let small = Grid::around([[-1000.0, -500.0, 0.0], [1000.0, 500.0, 0.0]]);
        assert_eq!(small.cell, MIN_CELL);
        assert_eq!(
            small.origin,
            [-0.5 * MIN_CELL * SIZE as f32, -0.5 * MIN_CELL * SIZE as f32]
        );
        // T2_Rogue's box: 16576 × 13816 units.
        let rogue = Grid::around([[-6168.0, -8856.0, 0.0], [10408.0, 4960.0, 0.0]]);
        assert_eq!(rogue.cell, 80.0);
        let reach = rogue.cell * SIZE as f32;
        assert!(rogue.origin[0] <= -6168.0 && rogue.origin[0] + reach >= 10408.0);
        assert!(rogue.origin[1] <= -8856.0 && rogue.origin[1] + reach >= 4960.0);
        assert_eq!(
            rogue.centre(SIZE + 2),
            [rogue.origin[0] + 200.0, rogue.origin[1] + 120.0]
        );
    }

    #[test]
    fn columns_beyond_the_walls_lie_at_the_middle_floor_under_the_highest_sky() {
        let mut columns = vec![Column::OUTSIDE_MAP; SIZE * SIZE];
        let open = |bottom: f32, top: f32| Column {
            bottom,
            top,
            flags: Column::SPLASH,
        };
        // A deep pit, two streets and two roofs, and a roofed column.
        columns[100] = open(-4000.0, 1000.0);
        columns[101] = open(-600.0, 1000.0);
        columns[102] = open(-600.0, 1000.0);
        columns[103] = open(300.0, 1200.0);
        columns[104] = open(500.0, 1000.0);
        columns[105] = Column::COVERED;
        fill_beyond(&mut columns);
        let beyond = Column {
            bottom: -600.0,
            top: 1200.0,
            flags: 0,
        };
        assert_eq!(columns[0], beyond);
        assert_eq!(columns[SIZE * SIZE - 1], beyond);
        assert_eq!(
            columns[103],
            open(300.0, 1200.0),
            "open columns keep their span"
        );
        assert_eq!(columns[105], Column::COVERED);
        // With nothing open there is no floor to take.
        let mut void = vec![Column::OUTSIDE_MAP; SIZE * SIZE];
        fill_beyond(&mut void);
        assert!(void.iter().all(|column| *column == Column::OUTSIDE_MAP));
    }
}
